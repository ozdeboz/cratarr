// The Noise source (decision 0057 §4, the owner 2026-10-20: noise and grain "together with filter can be used to great
// effect"): fractal simplex noise over the whole picture, as LZX's noise, Hydra's `noise()` and TouchDesigner's Noise
// TOP have it. Monochrome: light only, its colour a Tint's (§4g). At its finest scale, moving, it is new every frame:
// grain (no Grain module). Seeded by its module's id, so every device draws it alike. Nothing at its amount's neutral.
//
// The simplex noise is Ian McEwan's and Stefan Gustavson's (webgl-noise, github.com/stegu/webgl-noise, 2020-10-14), as
// it is there:
//
//   Copyright (C) 2011 by Ashima Arts (Simplex noise)
//   Copyright (C) 2011-2016 by Stefan Gustavson (Classic noise and others)
//
//   Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
//   documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
//   rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
//   permit persons to whom the Software is furnished to do so, subject to the following conditions:
//
//   The above copyright notice and this permission notice shall be included in all copies or substantial portions of
//   the Software.
//
//   THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
//   WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS
//   OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
//   OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
import type { FrameInfo, ModuleType, PictureModule } from '../patch';
import { done, program, target } from './gl';
import { knob, values } from './knob';

export const NOISE: ModuleType = {
  type: 'noise',
  label: 'Noise',
  place: 'source',
  knobs: [
    knob('amount', 'Amount', 0, 1, 1, 'level'),
    // 0 the finest (a cell about a pixel and a half: grain), 1 the coarsest (a cell the picture's height and a half).
    knob('scale', 'Scale', 0, 1, 0.5),
    // Octaves, 1 to 6; between whole numbers the last fades in.
    knob('detail', 'Detail', 1, 6, 3),
    // How fast it changes, by its own size: at the finest scale a new pattern each frame.
    knob('speed', 'Speed', 0, 1, 0.2),
    knob('contrast', 'Contrast', 0, 1, 0.5),
  ],
};

export const MOST_OCTAVES = 6;

/** A module's seed from its id: where in the noise it is (the same on every device). */
export function noiseSeed(id: string): [number, number, number] {
  let h = 2166136261;
  for (const ch of id) h = Math.imul(h ^ ch.charCodeAt(0), 16777619);
  const at = (k: number) => ((Math.imul(h ^ k, 2654435761) >>> 0) / 4294967296) * 200 - 100;
  return [at(1), at(2), at(3)];
}

/** What the shader draws with this frame (pure, so Node can test it): its cells across the shorter side, its octaves,
 *  where in time it is (by its own size, so the finest moves a pattern a frame), its contrast's gain and its amount. */
export function noiseNow(k: Record<string, number>, time: number, short: number) {
  const finest = Math.max(1, short / 1.5);
  const cells = Math.exp(Math.log(finest) + (Math.log(0.66) - Math.log(finest)) * k.scale);
  return {
    cells,
    octaves: Math.min(MOST_OCTAVES, Math.max(1, k.detail)),
    // Wrapped, so a float in the shader keeps its fractions (a pattern changes by a unit of z: at the coarsest and
    // slowest the wrap comes once in hours).
    z: (time * k.speed * 2 * cells) % 4096,
    gain: 2 ** (4 * (k.contrast - 0.5)),
    amount: k.amount,
  };
}

/** Where in time it is after `dt` more seconds from `z`, at its speed and scale now: run on a frame at a time, so a
 *  change of either (a cable, a macro, a morph) goes on from where it is (the owner, 2026-10-29: continuous controls;
 *  `noiseNow`'s z, the track's time times them, jumped at every change, the further the later in the track). */
export function noiseStep(z: number, k: Record<string, number>, dt: number, short: number): number {
  return (z + dt * k.speed * 2 * noiseNow(k, 0, short).cells) % 4096;
}

const VERTEX = `#version 300 es
void main() {
  vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0);
  gl_Position = vec4(p, 0.0, 1.0);
}`;

const FRAGMENT = `#version 300 es
precision highp float;
uniform vec2 uSize;
uniform float uCells;
uniform float uOctaves;
uniform float uZ;
uniform vec3 uSeed;
uniform float uGain;
uniform float uAmount;
layout(location = 0) out vec4 o;

// webgl-noise's 3D simplex noise (the notice in noise.ts).
vec3 mod289(vec3 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec4 mod289(vec4 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
vec4 permute(vec4 x) { return mod289(((x * 34.0) + 10.0) * x); }
vec4 taylorInvSqrt(vec4 r) { return 1.79284291400159 - 0.85373472095314 * r; }
float snoise(vec3 v) {
  const vec2 C = vec2(1.0 / 6.0, 1.0 / 3.0);
  const vec4 D = vec4(0.0, 0.5, 1.0, 2.0);
  vec3 i = floor(v + dot(v, C.yyy));
  vec3 x0 = v - i + dot(i, C.xxx);
  vec3 g = step(x0.yzx, x0.xyz);
  vec3 l = 1.0 - g;
  vec3 i1 = min(g.xyz, l.zxy);
  vec3 i2 = max(g.xyz, l.zxy);
  vec3 x1 = x0 - i1 + C.xxx;
  vec3 x2 = x0 - i2 + C.yyy;
  vec3 x3 = x0 - D.yyy;
  i = mod289(i);
  vec4 p = permute(permute(permute(i.z + vec4(0.0, i1.z, i2.z, 1.0)) + i.y + vec4(0.0, i1.y, i2.y, 1.0)) + i.x + vec4(0.0, i1.x, i2.x, 1.0));
  float n_ = 0.142857142857;
  vec3 ns = n_ * D.wyz - D.xzx;
  vec4 j = p - 49.0 * floor(p * ns.z * ns.z);
  vec4 x_ = floor(j * ns.z);
  vec4 y_ = floor(j - 7.0 * x_);
  vec4 x = x_ * ns.x + ns.yyyy;
  vec4 y = y_ * ns.x + ns.yyyy;
  vec4 h = 1.0 - abs(x) - abs(y);
  vec4 b0 = vec4(x.xy, y.xy);
  vec4 b1 = vec4(x.zw, y.zw);
  vec4 s0 = floor(b0) * 2.0 + 1.0;
  vec4 s1 = floor(b1) * 2.0 + 1.0;
  vec4 sh = -step(h, vec4(0.0));
  vec4 a0 = b0.xzyw + s0.xzyw * sh.xxyy;
  vec4 a1 = b1.xzyw + s1.xzyw * sh.zzww;
  vec3 p0 = vec3(a0.xy, h.x);
  vec3 p1 = vec3(a0.zw, h.y);
  vec3 p2 = vec3(a1.xy, h.z);
  vec3 p3 = vec3(a1.zw, h.w);
  vec4 norm = taylorInvSqrt(vec4(dot(p0, p0), dot(p1, p1), dot(p2, p2), dot(p3, p3)));
  p0 *= norm.x;
  p1 *= norm.y;
  p2 *= norm.z;
  p3 *= norm.w;
  vec4 m = max(0.5 - vec4(dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3)), 0.0);
  m = m * m;
  return 105.0 * dot(m * m, vec4(dot(p0, x0), dot(p1, x1), dot(p2, x2), dot(p3, x3)));
}

void main() {
  // Units: the shorter side is uCells cells; octaves of twice the cells at half the weight, the last by its share.
  vec2 at = gl_FragCoord.xy / min(uSize.x, uSize.y) * uCells;
  float sum = 0.0;
  float weights = 0.0;
  float f = 1.0;
  float w = 1.0;
  for (int k = 0; k < ${MOST_OCTAVES}; k++) {
    float share = clamp(uOctaves - float(k), 0.0, 1.0);
    if (share <= 0.0) break;
    sum += w * share * snoise(vec3(at * f, uZ) + uSeed + float(k) * 17.0);
    weights += w * share;
    f *= 2.0;
    w *= 0.5;
  }
  float v = clamp((sum / max(weights, 1e-4)) * 0.5 * uGain + 0.5, 0.0, 1.0) * uAmount;
  // Premultiplied white light.
  o = vec4(vec3(v), v);
}`;

export class Noise implements PictureModule {
  private program: WebGLProgram;
  private at: Record<string, WebGLUniformLocation | null>;
  private seed: [number, number, number];
  /** Where in time it is (noiseStep). */
  private z = 0;

  constructor(gl: WebGL2RenderingContext, id = '') {
    this.program = program(gl, VERTEX, FRAGMENT);
    this.at = Object.fromEntries(['uSize', 'uCells', 'uOctaves', 'uZ', 'uSeed', 'uGain', 'uAmount'].map((n) => [n, gl.getUniformLocation(this.program, n)]));
    this.seed = noiseSeed(id);
  }

  draw(gl: WebGL2RenderingContext, _input: WebGLTexture | null, out: WebGLFramebuffer | null, width: number, height: number, knobs: Record<string, number>, frame: FrameInfo) {
    const k = values(NOISE, knobs);
    this.z = noiseStep(this.z, k, frame.dt, Math.min(width, height));
    target(gl, out, width, height);
    if (k.amount <= 0) return;
    const now = noiseNow(k, frame.time, Math.min(width, height));
    gl.useProgram(this.program);
    gl.uniform2f(this.at.uSize, width, height);
    gl.uniform1f(this.at.uCells, now.cells);
    gl.uniform1f(this.at.uOctaves, now.octaves);
    gl.uniform1f(this.at.uZ, this.z);
    gl.uniform3f(this.at.uSeed, ...this.seed);
    gl.uniform1f(this.at.uGain, now.gain);
    gl.uniform1f(this.at.uAmount, now.amount);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    done(gl);
  }

  dispose(gl: WebGL2RenderingContext) {
    gl.deleteProgram(this.program);
  }
}
