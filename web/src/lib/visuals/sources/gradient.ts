// The Gradient source (the owner, 2026-10-29: "how do we get continuous controls instead of discrete controls?"): a
// field of light by place, as TouchDesigner's Ramp TOP and Hydra's gradient() give one, but one source whose kinds
// blend instead of a switch between them: the distance from a centre (radial), round it (in petals, any count, so with
// no seam), across and up (linear), each by an amount of its own, summed; banded by Repeat (a whole field at 0, bands
// fading in above it), the bands moving at Speed. Monochrome: light only, its colour a Tint's. Into a Displace's `by`
// it is a motion field by place (out from the centre, round it, across), into a Mask a vignette or a wipe, into a Tint
// the colours of a ramp. Nothing at its amount's neutral.
import type { FrameInfo, ModuleType, PictureModule } from '../patch';
import { done, program, target } from './gl';
import { knob, values } from './knob';
import { withAspects } from '../engine/knob-aspects';

export const GRADIENT: ModuleType = {
  type: 'gradient',
  label: 'Gradient',
  place: 'source',
  knobs: withAspects([
    knob('amount', 'Amount', 0, 1, 1, 'level'),
    // How much of each field (below 0 turned over): the distance from the centre, 0 there and 1 at the short side's edge.
    knob('distance', 'Distance', -1, 1, 1),
    // Round the centre: petals of light, Round count of them (between two whole ones the next fades in).
    knob('round', 'Round', -1, 1, 0),
    knob('roundCount', 'Round count', 0, 12, 1),
    // Across and up the picture, 0 at the middle.
    knob('across', 'Across', -1, 1, 0),
    knob('up', 'Up', -1, 1, 0),
    // The centre, and the fields turned about it (in turns).
    knob('centreX', 'Centre across', -1, 1, 0),
    knob('centreY', 'Centre up', -1, 1, 0),
    knob('turn', 'Turn', -1, 1, 0),
    // Bands: the field as it is at 0, then bands of it, this many to the field's unit, fading in up to one.
    knob('repeat', 'Repeat', 0, 16, 0),
    // How fast the bands move along the field (bands a second; below 0 the other way).
    knob('speed', 'Speed', -1, 1, 0),
  ], {
    amount: null, distance: 'spatial', round: 'spatial', roundCount: 'spatial', across: 'spatial', up: 'spatial', centreX: 'spatial', centreY: 'spatial',
    turn: 'spatial', repeat: 'spatial', speed: 'motion',
  }),
};

/** Its fields at the point `p` (centred, aspect-true: the short side −1..1, the long side ±aspect), as the shader draws
 *  them, so Node can test it: the light there (0..1), at its bands' place `phase` (turns). */
export function gradientAt(p: [number, number], k: Record<string, number>, aspect: number, phase: number): number {
  const [cx, cy] = centreOf(k, aspect);
  const t = -k.turn * 2 * Math.PI;
  const dx = p[0] - cx;
  const dy = p[1] - cy;
  const x = Math.cos(t) * dx - Math.sin(t) * dy;
  const y = Math.sin(t) * dx + Math.cos(t) * dy;
  const r = Math.hypot(x, y);
  const th = r > 1e-6 ? Math.atan2(y, x) : 0;
  const n0 = Math.floor(Math.max(0, k.roundCount));
  const f = Math.max(0, k.roundCount) - n0;
  const round = 0.5 + 0.5 * ((1 - f) * Math.cos(n0 * th) + f * Math.cos((n0 + 1) * th));
  const field = k.distance * r + k.round * round + k.across * 0.5 * x + k.up * 0.5 * y;
  return k.amount * banded(field, k.repeat, phase);
}

/** The centre in the fields' units (the short side −1..1) on a picture `aspect` wide to 1 tall: Centre across and up
 *  each reach their own edge. */
export function centreOf(k: Record<string, number>, aspect: number): [number, number] {
  return aspect >= 1 ? [k.centreX * aspect, k.centreY] : [k.centreX, k.centreY / aspect];
}

/** The field `v` in bands, `repeat` of them to its unit (a cosine's, so they meet smoothly), moved by `phase`: at 0 the
 *  field as it is (clamped to 0..1), fading into the bands up to one. */
export function banded(v: number, repeat: number, phase: number): number {
  const plain = Math.min(1, Math.max(0, v));
  if (repeat <= 0) return plain;
  const bands = 0.5 - 0.5 * Math.cos(2 * Math.PI * (Math.max(1, repeat) * v - phase));
  return plain + (bands - plain) * Math.min(1, repeat);
}

const VERTEX = `#version 300 es
void main() {
  vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0);
  gl_Position = vec4(p, 0.0, 1.0);
}`;

// No arrays (Mali's float[] miscompiles); atan only off the centre (atan(0, 0) is undefined on some GPUs).
const FRAGMENT = `#version 300 es
precision highp float;
uniform vec2 uSize;
uniform vec4 uWeights;
uniform vec2 uCentre;
uniform float uTurn;
uniform float uCount;
uniform vec3 uBands;
layout(location = 0) out vec4 o;
void main() {
  float side = min(uSize.x, uSize.y);
  vec2 p = (gl_FragCoord.xy - 0.5 * uSize) / (0.5 * side) - uCentre;
  float c = cos(uTurn), s = sin(uTurn);
  p = vec2(c * p.x - s * p.y, s * p.x + c * p.y);
  float r = length(p);
  float th = r > 1e-6 ? atan(p.y, p.x) : 0.0;
  float n0 = floor(uCount);
  float f = uCount - n0;
  float round = 0.5 + 0.5 * mix(cos(n0 * th), cos((n0 + 1.0) * th), f);
  float v = uWeights.x * r + uWeights.y * round + uWeights.z * 0.5 * p.x + uWeights.w * 0.5 * p.y;
  float plain = clamp(v, 0.0, 1.0);
  float bands = 0.5 - 0.5 * cos(6.28318531 * (max(1.0, uBands.x) * v - uBands.y));
  float light = mix(plain, bands, min(1.0, uBands.x)) * uBands.z;
  // Premultiplied white light.
  o = vec4(vec3(light), light);
}`;

export class Gradient implements PictureModule {
  private program: WebGLProgram;
  private at: Record<string, WebGLUniformLocation | null>;
  /** Its bands' place (turns), run on by its speed a frame at a time: a change of speed goes on from where they are. */
  private phase = 0;

  constructor(gl: WebGL2RenderingContext) {
    this.program = program(gl, VERTEX, FRAGMENT);
    this.at = Object.fromEntries(['uSize', 'uWeights', 'uCentre', 'uTurn', 'uCount', 'uBands'].map((n) => [n, gl.getUniformLocation(this.program, n)]));
  }

  draw(gl: WebGL2RenderingContext, _input: WebGLTexture | null, out: WebGLFramebuffer | null, width: number, height: number, knobs: Record<string, number>, frame: FrameInfo) {
    const k = values(GRADIENT, knobs);
    this.phase = (this.phase + k.speed * frame.dt) % 1;
    target(gl, out, width, height);
    if (k.amount <= 0) return;
    gl.useProgram(this.program);
    gl.uniform2f(this.at.uSize, width, height);
    gl.uniform4f(this.at.uWeights, k.distance, k.round, k.across, k.up);
    gl.uniform2f(this.at.uCentre, ...centreOf(k, width / Math.max(1, height)));
    gl.uniform1f(this.at.uTurn, -k.turn * 2 * Math.PI);
    gl.uniform1f(this.at.uCount, Math.max(0, k.roundCount));
    gl.uniform3f(this.at.uBands, Math.max(0, k.repeat), this.phase, k.amount);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    done(gl);
  }

  dispose(gl: WebGL2RenderingContext) {
    gl.deleteProgram(this.program);
  }
}
