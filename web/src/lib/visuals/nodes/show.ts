// The light shown (decision 0046, 0057 step 3: Show's device, Bend, Blur, Tone map, Curve and Saturate, fused): the
// paper's log-density display of the light, read where the waves bend it or along the fluid's flow, smoothed where
// few points fell, its gamma and floor, the bands' lights and glow, the fire, the colour and the ceiling.
import { PARTS } from '../flame';
import { WAVE_BEND, type Fluid } from '../fluid';
import { FIELD_GLSL, GIANT_GLSL } from '../field';
import { program } from '../sources/gl';
import type { ShowParams } from '../stage-values';
import type { Light } from './light';
import type { FlamePoints } from './points';
import { BAD, FULL, none, uniforms } from './shared';

const SHOW = `#version 300 es
precision highp float;
uniform sampler2D uAcc;
uniform vec2 uView;
uniform float uK1;
uniform float uK2;
uniform float uGamma;
uniform float uVibrancy;
uniform float uZoom;
uniform float uFlash;
uniform sampler2D uBandsA;
uniform sampler2D uBandsB;
// Each band's light now (-1..3: above 0 its transients', below it dimmed), and its colour.
uniform float uHit[${PARTS}];
uniform vec3 uBandRgb[${PARTS}];
uniform float uCeiling;
uniform float uSaturation;
// Smoothing: the most a pixel where few points fell is spread over (pixels of the buffer), and the buffer's levels
// above its first that it may take that from (none where it has none). Floor: the faintest density shown (on the
// display's scale, a.a * uK2; 0 for all). The fire's share: a blackbody's heat where the points are dense.
uniform float uSmooth;
uniform float uLevels;
uniform float uFloor;
uniform float uFire;
// The fluid's flow (cells a second, its grid's), the seconds of it the picture is read back along (0: none), and a
// cell's size in texture coordinates.
uniform sampler2D uVelocity;
uniform float uWarp;
// The giant waves where nothing flows (fluid.ts, field.ts): the picture read back along their flow, at their phase.
uniform vec2 uWave;
uniform vec2 uUvPerCell;
out vec4 o;
${BAD}
${FIELD_GLSL}
${GIANT_GLSL}
// A sample of the buffer, none of it anything but a number (whatever a driver did to it before the scrub came by).
vec4 at(vec2 uv, float lod) { return clean(textureLod(uAcc, uv, lod)); }
vec4 raw(vec2 uv, float lod) { return textureLod(uAcc, uv, lod); }
// A blackbody's colours by its heat (0..1): black, a deep red, orange, yellow, white only at the very hottest.
vec3 heat(float t) { return clamp(vec3(2.2 * t, 2.2 * t - 0.8, 4.0 * t - 3.3), 0.0, 1.0); }
void main() {
  // A beat's swell: the picture drawn a little larger about its middle.
  vec2 uv = 0.5 + (gl_FragCoord.xy / uView - 0.5) / uZoom;
  if (uWarp > 0.0) uv -= uWarp * textureLod(uVelocity, uv, 0.0).xy * uUvPerCell;
  if (uWave.x != 0.0) uv += giantAt(uv, kOf(uView.x / uView.y), uWave.y) * uWave.x;
  vec2 px = 1.0 / vec2(textureSize(uAcc, 0));
  vec4 a = at(uv, 0.0);
  // The pixel's points by band: shares of one.
  vec4 shareA = clamp(clean(textureLod(uBandsA, uv, 0.0)) / max(a.a, 1e-6), 0.0, 1.0);
  vec2 shareB = clamp(clean(textureLod(uBandsB, uv, 0.0)).xy / max(a.a, 1e-6), 0.0, 1.0);
  // Element by element: the Mali-G615's compiler refuses an array constructor here ("no default precision defined for
  // variable 'float[6]'", 2026-10-18), which left that phone without visuals.
  float share[${PARTS}];
  share[0] = shareA.x;
  share[1] = shareA.y;
  share[2] = shareA.z;
  share[3] = shareA.w;
  share[4] = shareB.x;
  share[5] = shareB.y;
  // The paper's density estimation, cheaply: where few points fell, the pixel is the mean of a wider ring around it
  // (up to uSmooth pixels out; past three, from a coarser level of the buffer, so the ring is a haze rather than
  // eight copies), where many fell it stays sharp. The ring is cleaned once, as a whole: a value not a number in it
  // costs that pixel its ring for a frame (the scrub clears it from the buffer), and cleaning each of its eight was
  // a third of a tablet's frame (Mali-G52: 13.6 ms to 9.3 ms, 2026-10-18).
  float radius = uSmooth / (1.0 + sqrt(a.a * uK2) * 2.0);
  if (radius > 0.4) {
    float lod = min(uLevels, max(0.0, log2(radius / 3.0)));
    vec2 d = px * radius;
    vec4 ring = raw(uv + vec2(d.x, 0.0), lod) + raw(uv - vec2(d.x, 0.0), lod)
              + raw(uv + vec2(0.0, d.y), lod) + raw(uv - vec2(0.0, d.y), lod)
              + raw(uv + d * 0.7071, lod) + raw(uv - d * 0.7071, lod)
              + raw(uv + vec2(d.x, -d.y) * 0.7071, lod) + raw(uv + vec2(-d.x, d.y) * 0.7071, lod);
    a = (a + clean(ring)) / 9.0;
  }
  if (a.a <= 0.0) { o = vec4(0.0, 0.0, 0.0, 1.0); return; }
  float x = a.a * uK2;
  float ls = uK1 * log(1.0 + x) / a.a;
  vec3 c = a.rgb * ls;
  float alpha = a.a * ls;
  // flam3's gamma threshold: below it the curve is a straight line, so the faintest pixels are not lifted into noise.
  const float T = 0.02;
  float g = alpha < T ? alpha / T * pow(T, 1.0 / uGamma) : pow(min(alpha, 1.0), 1.0 / uGamma);
  vec3 vib = c * (g / max(alpha, 1e-6));
  vec3 flat_ = pow(max(c, 0.0), vec3(1.0 / uGamma));
  // A beat's flash, after the gamma, so it shows as much in the faint parts as in the bright ones; a band's light
  // brightens the points of its colour and adds a glow of that colour, or (below 0) dims them, to 0.4 at -1. The
  // shares are of one, so the bands' lights are a mean here, never a sum.
  float lit = 0.0;
  vec3 glow = vec3(0.0);
  for (int k = 0; k < ${PARTS}; k++) {
    float h = uHit[k] * share[k];
    lit += h;
    glow += max(h, 0.0) * uBandRgb[k];
  }
  vec3 shown = mix(flat_, vib, uVibrancy);
  // Fire: the denser, the hotter, over the bands' own colours (which still tell them apart).
  shown = mix(shown, heat(g), 0.65 * uFire);
  shown = shown * uFlash * (1.0 + 0.6 * lit) + 0.2 * g * glow;
  // Floor: fainter than it, black (from half of it to twice it, so its edge does not crawl with the grain).
  if (uFloor > 0.0) shown *= smoothstep(-1.0, 1.0, log2(x / uFloor));
  // Less colour where the music is quiet (towards its grey of the same brightness).
  shown = mix(vec3(dot(shown, vec3(0.299, 0.587, 0.114))), shown, uSaturation);
  // The most brightness shown, last: no flash or light goes past it.
  o = vec4(clamp(shown, 0.0, 1.0) * uCeiling, 1.0);
}`;

/** The light's density on the display's scale (the show's k2): the points held at a time (each frame's `ready`, `passes`
 *  steps, kept 1/(1 − keep) frames), their mean per pixel, at the light's precision and the exposure. The Fluid's Lift
 *  reads its heat on the same scale (fluid.ts Heat). */
export function densityScale(light: Pick<Light, 'width' | 'height' | 'precision'>, ready: number, passes: number, keep: number, exposure: number): number {
  const held = (Math.max(1, ready) * passes) / Math.max(0.02, 1 - keep);
  const perPixel = held / (light.width * light.height);
  return ((light.precision === 8 ? 255 : 4) * exposure) / perPixel;
}

export class Show {
  private gl: WebGL2RenderingContext;
  private show: WebGLProgram;
  private empty: WebGLVertexArrayObject;
  private u: (p: WebGLProgram, name: string) => WebGLUniformLocation | null;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
    this.u = uniforms(gl);
    this.show = program(gl, FULL, SHOW);
    this.empty = gl.createVertexArray()!;
  }

  /** The light (`points`'s in `light`, `passes` steps this frame) shown into the target bound, `viewWidth` x
   *  `viewHeight`; `fluid` the one it was carried along, its read-back `warpShown`. */
  draw(light: Light, points: FlamePoints, look: ShowParams, passes: number, fluid: Fluid | null, warpShown: number, viewWidth: number, viewHeight: number) {
    const gl = this.gl;
    gl.viewport(0, 0, viewWidth, viewHeight);
    gl.useProgram(this.show);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, light.acc);
    gl.uniform1i(this.u(this.show, 'uAcc'), 0);
    gl.uniform2f(this.u(this.show, 'uView'), viewWidth, viewHeight);
    gl.uniform1f(this.u(this.show, 'uK1'), look.brightness);
    gl.uniform1f(this.u(this.show, 'uK2'), densityScale(light, points.ready, passes, look.keep, look.exposure));
    gl.uniform1f(this.u(this.show, 'uGamma'), look.gamma);
    gl.uniform1f(this.u(this.show, 'uVibrancy'), look.vibrancy);
    gl.uniform1f(this.u(this.show, 'uZoom'), look.zoom);
    gl.uniform1f(this.u(this.show, 'uFlash'), look.flash);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, light.bands);
    gl.uniform1i(this.u(this.show, 'uBandsA'), 1);
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, light.bandsB);
    gl.uniform1i(this.u(this.show, 'uBandsB'), 2);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform1fv(this.u(this.show, 'uHit'), Float32Array.from({ length: PARTS }, (_, k) => look.hits[k] ?? 0));
    gl.uniform3fv(this.u(this.show, 'uBandRgb'), points.bandRgb);
    gl.uniform1f(this.u(this.show, 'uCeiling'), look.ceiling);
    gl.uniform1f(this.u(this.show, 'uSaturation'), look.saturation);
    // A smoothing wider than three pixels takes its ring from the buffer's coarser levels, made now (only then: the
    // levels cost a pass over the buffer).
    const levels = look.smoothing > 3 ? light.levels : 0;
    if (levels) {
      gl.bindTexture(gl.TEXTURE_2D, light.acc);
      gl.generateMipmap(gl.TEXTURE_2D);
    }
    gl.uniform1f(this.u(this.show, 'uSmooth'), look.smoothing);
    gl.uniform1f(this.u(this.show, 'uLevels'), levels);
    gl.uniform1f(this.u(this.show, 'uFloor'), look.floor);
    gl.uniform1f(this.u(this.show, 'uFire'), look.fire);
    const warp = fluid ? warpShown : 0;
    gl.uniform1f(this.u(this.show, 'uWarp'), warp);
    // Where the fluid flows it carries the waves itself; else the picture is bent by them.
    const wave = fluid ? undefined : look.flow?.wave;
    gl.uniform2f(this.u(this.show, 'uWave'), wave ? WAVE_BEND * wave.amount : 0, wave ? wave.phase : 0);
    // The flow's unit is sampled with or without one: a black pixel there without.
    gl.activeTexture(gl.TEXTURE3);
    gl.bindTexture(gl.TEXTURE_2D, fluid && warp ? fluid.velocity : none(gl));
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform1i(this.u(this.show, 'uVelocity'), 3);
    if (fluid && warp) gl.uniform2fv(this.u(this.show, 'uUvPerCell'), fluid.uvPerCell);
    gl.bindVertexArray(this.empty);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  dispose() {
    this.gl.deleteVertexArray(this.empty);
    this.gl.deleteProgram(this.show);
  }
}
