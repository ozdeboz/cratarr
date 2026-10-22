// The picture modules a patch wires freely (decision 0057 §4, §5, §6b), each one pass from its picture (and `by`, a
// second picture, where it takes one) into a target: Glow (its own light spread round it), Tint (its colours the
// hue of `by` round it), Mask (dimmed where `by` is not: 70.1's window), Displace (read where `by` pushes it),
// Threshold (only its bright parts: Bloom's), Blur, Bend, Curve and Saturate on a picture (on the light they are
// Show's own pass), and Warp and Keep on their own, Transform on a picture (transform.ts).
// Glow's, Tint's and Mask's maths are the Mix's (compose.ts), so a module drawn there or here gives the same. Taps one
// by one, no arrays, level-0 reads where the read is moved (the Mali rules).
import type { FrameInfo, PictureModule } from '../patch';
import { done, program } from '../sources/gl';
import { WAVE_BEND } from '../fluid';
import { FIELD_GLSL, GIANT_GLSL } from '../field';
import { keptPerSecond } from '../loopmods';
import { inverse, toScreen, transformOf } from '../transform';
import { none } from './shared';

/** A module's pass: its picture inputs by port (`picture`, `by`), drawn into `out` at `width` × `height`; the points
 *  cabled into it (Instance's, Fold's grid: points-pictures.ts), where it reads them. */
/** A flow field as a node reads it: the Fluid's (its velocity texture, cells a second, and texture units per cell) or a
 *  Field's (field.ts: its scale and time run on). */
export type FlowField = { tex: WebGLTexture; uvPerCell: [number, number] } | { scale: number; phase: number; waves?: readonly number[] };

export type ModuleNode = {
  draw(gl: WebGL2RenderingContext, inputs: { picture: WebGLTexture | null; by?: WebGLTexture | null; field?: FlowField | null; windows?: readonly (WebGLTexture | null)[]; along?: WebGLTexture | null }, out: WebGLFramebuffer, width: number, height: number, knobs: Record<string, number>, frame: FrameInfo, points?: readonly { x: number; y: number }[] | null): void;
  dispose(gl: WebGL2RenderingContext): void;
};

const VERTEX = `#version 300 es
out vec2 vUv;
void main() {
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const HEAD = `#version 300 es
precision highp float;
in vec2 vUv;
out vec4 o;
uniform sampler2D uIn;
uniform sampler2D uBy;
uniform vec2 uAspect;
uniform vec2 uPx;
vec4 ring(sampler2D t, float r) {
  vec2 d = uAspect * r;
  float h = 0.7071;
  return texture(t, vUv + vec2(d.x, 0.0)) + texture(t, vUv - vec2(d.x, 0.0)) + texture(t, vUv + vec2(0.0, d.y)) +
    texture(t, vUv - vec2(0.0, d.y)) + texture(t, vUv + d * h) + texture(t, vUv - d * h) +
    texture(t, vUv + vec2(d.x, -d.y) * h) + texture(t, vUv + vec2(-d.x, d.y) * h);
}
float luma(vec3 c) { return dot(c, vec3(0.2126, 0.7152, 0.0722)); }
`;

/** Each module's fragment shader body (after HEAD), and its uniforms from its knobs. */
const PASSES: Record<string, { glsl: string; uniforms: (k: Record<string, number>, frame: FrameInfo, w: number, h: number) => Record<string, number | [number, number] | [number, number, number] | [number, number, number, number]> }> = {
  // Each pixel read where the Transform brought it from (its map back), mirrored past the edges as the shapes read.
  transform: {
    glsl: `uniform vec4 uBack;
uniform vec2 uBackAt;
uniform vec2 uK;
void main() {
  vec2 from = (mat2(uBack.xy, uBack.zw) * ((vUv * 2.0 - 1.0) / uK) + uBackAt) * uK * 0.5 + 0.5;
  o = textureLod(uIn, 1.0 - abs(1.0 - mod(from, 2.0)), 0.0);
}`,
    uniforms: (k, _frame, w, h) => {
      const back = inverse(transformOf(k, w / h));
      return { uBack: back.m, uBackAt: back.at, uK: toScreen(w / h) };
    },
  },
  glow: {
    glsl: `uniform float uGlow;
void main() {
  vec4 b = texture(uIn, vUv);
  if (uGlow > 0.0) b += (ring(uIn, 0.012) * 0.5 + ring(uIn, 0.03) * 0.3) / 8.0 * 2.5 * uGlow;
  o = b;
}`,
    uniforms: (k) => ({ uGlow: k.glow ?? 0 }),
  },
  tint: {
    glsl: `uniform float uTint;
void main() {
  vec4 b = texture(uIn, vUv);
  if (uTint > 0.0) {
    vec3 m = ring(uBy, 0.06).rgb / 8.0;
    float top = max(max(m.r, m.g), m.b);
    float lit = max(max(b.r, b.g), b.b);
    b.rgb = mix(b.rgb, lit * m / max(top, 1e-3), uTint * smoothstep(0.01, 0.12, top));
  }
  o = b;
}`,
    uniforms: (k) => ({ uTint: k.tint ?? 0 }),
  },
  mask: {
    glsl: `uniform float uAmount;
uniform float uInvert;
void main() {
  vec4 b = texture(uBy, vUv);
  float cover = clamp(max(max(b.r, b.g), max(b.b, b.a)) * 1.5, 0.0, 1.0);
  if (uInvert > 0.5) cover = 1.0 - cover;
  o = texture(uIn, vUv) * mix(1.0 - uAmount, 1.0, cover);
}`,
    uniforms: (k) => ({ uAmount: k.amount ?? 0, uInvert: k.invert ?? 0 }),
  },
  displace: {
    glsl: `uniform float uAmount;
uniform float uScale;
uniform float uMode;
void main() {
  vec2 d;
  if (uMode > 0.5) d = texture(uBy, vUv).rg * 2.0 - 1.0;
  else {
    // The way \`by\`'s brightness rises, over a span its Scale sets (a pixel to a twentieth of the picture).
    vec2 e = mix(uPx, uAspect * 0.05, uScale);
    d = vec2(luma(texture(uBy, vUv + vec2(e.x, 0.0)).rgb) - luma(texture(uBy, vUv - vec2(e.x, 0.0)).rgb),
             luma(texture(uBy, vUv + vec2(0.0, e.y)).rgb) - luma(texture(uBy, vUv - vec2(0.0, e.y)).rgb));
    d = clamp(d * 4.0, -1.0, 1.0);
  }
  o = textureLod(uIn, vUv + uAmount * 0.05 * d * uAspect, 0.0);
}`,
    uniforms: (k) => ({ uAmount: k.amount ?? 0, uScale: k.scale ?? 0.5, uMode: k.mode ?? 0 }),
  },
  threshold: {
    glsl: `uniform float uThreshold;
uniform float uSoftness;
void main() {
  vec4 b = texture(uIn, vUv);
  // Brighter than the threshold all of it, fading out over Softness's share below it; at 0 everything passes.
  float lo = min(uThreshold * (1.0 - uSoftness), uThreshold - 1e-4);
  o = uThreshold <= 0.0 ? b : b * smoothstep(lo, uThreshold, luma(b.rgb / max(b.a, 1.0)));
}`,
    uniforms: (k) => ({ uThreshold: k.threshold ?? 0, uSoftness: k.softness ?? 0.1 }),
  },
  // Blur on a picture: the mean of it and two rings round it, as wide as Show's smoothing reaches (12 Smoothing²
  // pixels at most).
  blur: {
    glsl: `uniform float uRadius;
void main() {
  vec4 b = texture(uIn, vUv);
  if (uRadius > 0.4) {
    float r = uRadius * min(uPx.x / uAspect.x, uPx.y / uAspect.y);
    b = (b + ring(uIn, r) + ring(uIn, r * 0.5)) / 17.0;
  }
  o = b;
}`,
    uniforms: (k) => ({ uRadius: 12 * (k.smoothing ?? 0) ** 2 }),
  },
  // Bend on a picture: the slow giant waves (fluid.ts) bending it across their way as their crest passes, rolling on
  // with the frame's time.
  bend: {
    glsl: `uniform vec2 uWave;
${FIELD_GLSL}
${GIANT_GLSL}
void main() {
  vec2 uv = vUv;
  if (uWave.x != 0.0) uv += giantAt(uv, kOf(uAspect.y / uAspect.x), uWave.y) * uWave.x;
  o = textureLod(uIn, uv, 0.0);
}`,
    uniforms: (k, frame) => ({ uWave: [WAVE_BEND * (k.waves ?? 0), frame.time] }),
  },
  // Curve on a picture (Show's Haze as a module of its own): Show's gamma is 3 × 2^haze, so on a picture already shown
  // at 3 the rest of it, 2^haze; below 0 a floor too, the faintest (by brightness) black.
  curve: {
    glsl: `uniform float uGamma;
uniform float uFloor;
void main() {
  vec4 b = texture(uIn, vUv);
  vec3 c = pow(max(b.rgb, 0.0), vec3(1.0 / uGamma));
  if (uFloor > 0.0) c *= smoothstep(0.5 * uFloor, 2.0 * uFloor, luma(c));
  o = vec4(c, b.a);
}`,
    uniforms: (k) => {
      const h = Math.min(1, Math.max(-1, k.haze ?? 0));
      return { uGamma: 2 ** h, uFloor: h < 0 ? 0.15 * -h : 0 };
    },
  },
  // Saturate on a picture: towards its grey of the same brightness (Show's weights), or past its colour; at 0 as it is.
  saturate: {
    glsl: `uniform float uSaturation;
void main() {
  vec4 b = texture(uIn, vUv);
  o = vec4(mix(vec3(dot(b.rgb, vec3(0.299, 0.587, 0.114))), b.rgb, uSaturation), b.a);
}`,
    uniforms: (k) => ({ uSaturation: Math.max(0, 1 + (k.saturation ?? 0)) }),
  },
  // Warp and Keep on their own (the picture's own loop as modules: compose.ts draws the usual loop in one pass): the
  // picture read where the Warp moves it from about the middle (a second's worth at full: half again larger or
  // smaller, half a turn, a fifth of the picture across), and kept by the Keep's share.
  warp: {
    glsl: `uniform float uZoom;
uniform float uTurn;
uniform vec2 uDrift;
uniform float uStretch;
uniform vec3 uWobble;
void main() {
  float aspect = uAspect.y / uAspect.x;
  vec2 p = (vUv - 0.5) * vec2(aspect, 1.0);
  p /= uZoom;
  float c = cos(uTurn), s = sin(uTurn);
  p = vec2(c * p.x - s * p.y, s * p.x + c * p.y);
  p *= vec2(1.0 / uStretch, uStretch);
  p += uWobble.x * vec2(sin(p.y * uWobble.y + uWobble.z), cos(p.x * uWobble.y - uWobble.z));
  o = textureLod(uIn, p / vec2(aspect, 1.0) + 0.5 - uDrift, 0.0);
}`,
    uniforms: (k, frame) => {
      const a = k.amount ?? 0;
      const dt = frame.dt;
      return {
        uZoom: Math.exp(0.4 * a * (k.zoom ?? 0) * dt),
        uTurn: Math.PI * a * (k.turn ?? 0) * dt,
        uDrift: [0.2 * a * (k.driftX ?? 0) * dt, 0.2 * a * (k.driftY ?? 0) * dt],
        uStretch: Math.exp(0.2 * a * (k.stretch ?? 0) * dt),
        uWobble: [0.02 * a * (k.wobble ?? 0) * Math.min(1, dt * 30), 4 + 20 * (k.wobbleSize ?? 0.5), frame.time * 1.7],
      };
    },
  },
  keep: {
    glsl: `uniform float uKeep;
void main() { o = texture(uIn, vUv) * uKeep; }`,
    // At nothing it keeps nothing (the loop gone: the brighter of the new picture and nothing is the new one).
    uniforms: (k, frame) => ({ uKeep: (k.amount ?? 0) > 0 ? 1 - k.amount + k.amount * keptPerSecond(k.trails ?? 0.5) ** frame.dt : 0 }),
  },
};

/** The module types drawn here. */
export const PICTURE_NODES: ReadonlySet<string> = new Set(Object.keys(PASSES));

/** A running module of `type`, drawn here. */
export function createNode(type: string, gl: WebGL2RenderingContext): ModuleNode {
  const pass = PASSES[type];
  if (!pass) throw new Error(`No such module: ${type}`);
  const p = program(gl, VERTEX, HEAD + pass.glsl);
  const locs = new Map<string, WebGLUniformLocation | null>();
  const at = (n: string) => {
    if (!locs.has(n)) locs.set(n, gl.getUniformLocation(p, n));
    return locs.get(n)!;
  };
  const vao = gl.createVertexArray()!;
  return {
    draw(gl, inputs, out, width, height, knobs, frame) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, out);
      gl.viewport(0, 0, width, height);
      gl.disable(gl.BLEND);
      gl.useProgram(p);
      [inputs.picture, inputs.by ?? null].forEach((t, i) => {
        gl.activeTexture(gl.TEXTURE0 + i);
        gl.bindTexture(gl.TEXTURE_2D, t ?? none(gl));
      });
      gl.activeTexture(gl.TEXTURE0);
      gl.uniform1i(at('uIn'), 0);
      gl.uniform1i(at('uBy'), 1);
      gl.uniform2f(at('uAspect'), Math.min(1, height / width), Math.min(1, width / height));
      gl.uniform2f(at('uPx'), 1 / width, 1 / height);
      for (const [n, v] of Object.entries(pass.uniforms(knobs, frame, width, height))) {
        if (typeof v === 'number') gl.uniform1f(at(n), v);
        else if (v.length === 2) gl.uniform2f(at(n), v[0], v[1]);
        else if (v.length === 3) gl.uniform3f(at(n), v[0], v[1], v[2]);
        else gl.uniform4f(at(n), v[0], v[1], v[2], v[3]);
      }
      gl.bindVertexArray(vao);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      gl.bindVertexArray(null);
      done(gl);
    },
    dispose(gl) {
      gl.deleteProgram(p);
      gl.deleteVertexArray(vao);
    },
  };
}

/** A picture module of one input (a source's, a layer's: patch.ts) as a node. */
export const nodeOf = (m: PictureModule): ModuleNode => ({
  draw: (gl, inputs, out, width, height, knobs, frame) => m.draw(gl, inputs.picture, out, width, height, knobs, frame),
  dispose: (gl) => m.dispose(gl),
});
