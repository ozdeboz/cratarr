// The picture's own loop (decision 0056): after the layers are blended, the last frame's picture carried along and
// kept, then the new one added: MilkDrop's warp and decay, as two modules. Each has an Amount whose neutral (0) does
// nothing, so the loop morphs in and out; every other knob swings about its default, where it does nothing either.
// Their drawing is the compositor's (compose.ts). Rates are a second's, so the picture moves alike at any frame rate.

import type { Knob, ModuleType } from './patch';

const knob = (id: string, label: string, min: number, max: number, def: number, kind: 'level' | 'swing' = 'swing'): Knob => ({
  id,
  label,
  min,
  max,
  neutral: kind === 'level' ? min : def,
  default: def,
  kind,
});

/** Warp: the last picture moved before it is kept. Zoom (in, out), turn, drift across, stretch, and a wobble that
 *  bends it as water does; their rates a second. And MilkDrop's grid (its per-vertex warp; the owner, 2026-10-29: "we're
 *  lacking a quality that MilkDrop has"): how the zoom and the turn change from place to place about a centre, more
 *  towards the edge, in petals round it, in rings running out from it, so the picture flows, tunnels and swirls as a
 *  whole rather than moving alike everywhere. Every knob is continuous (a petal count between two whole ones fades the
 *  next petal in); each does nothing at its default, but the counts and speeds, which wait on their amounts. */
export const WARP: ModuleType = {
  type: 'warp',
  label: 'Warp',
  place: 'loop',
  knobs: [
    knob('amount', 'Amount', 0, 1, 1, 'level'),
    knob('zoom', 'Zoom', -1, 1, 0),
    knob('turn', 'Turn', -1, 1, 0),
    knob('driftX', 'Drift across', -1, 1, 0),
    knob('driftY', 'Drift up', -1, 1, 0),
    knob('stretch', 'Stretch', -1, 1, 0),
    knob('wobble', 'Wobble', 0, 1, 0),
    knob('wobbleSize', 'Wobble size', 0, 1, 0.5),
    knob('centreX', 'Centre across', -1, 1, 0),
    knob('centreY', 'Centre up', -1, 1, 0),
    knob('zoomEdge', 'Zoom at the edge', -1, 1, 0),
    knob('turnEdge', 'Turn at the edge', -1, 1, 0),
    knob('petals', 'Petals', -1, 1, 0),
    knob('petalCount', 'Petal count', 0, 12, 4),
    knob('petalTurn', 'Petal turn', -1, 1, 0),
    knob('rings', 'Rings', -1, 1, 0),
    knob('ringCount', 'Ring count', 0.5, 12, 3),
    knob('ringSpeed', 'Ring speed', -1, 1, 0.5),
  ],
};

/** Keep: how much of the last picture stays (Amount; 0 none, as without a loop) and how long its trails last. */
export const KEEP: ModuleType = {
  type: 'keep',
  label: 'Keep',
  place: 'loop',
  knobs: [knob('amount', 'Amount', 0, 1, 1, 'level'), knob('trails', 'Trails', 0, 1, 0.5)],
};

export const LOOP_TYPES: ModuleType[] = [WARP, KEEP];

/** Keep's Trails (0..1) as the share of the picture kept a second: from a tenth of a second's trail to six seconds'. */
export const keptPerSecond = (trails: number) => Math.exp(-1 / (0.1 * 60 ** Math.min(1, Math.max(0, trails))));

// ---- The Warp's grid: one place for the loop drawn in one pass (compose.ts LOOP) and the Warp drawn on its own
// (nodes/modules.ts) ----

const TAU = Math.PI * 2;

/** A frame's Warp, its Warps' knobs summed by their amounts (a loop draws its Warps as one): the logarithm of this
 *  frame's zoom about the centre by place (`zoom`: everywhere; then times the distance from the centre, times the
 *  petals' shape round it, times the rings'), this frame's turn (everywhere, and times the distance), the centre (in
 *  the shader's p: the short side −0.5..0.5, the long side ±aspect/2), the stretch, the drift and the wobble as before,
 *  the petals' count and their turn so far, the rings' count across the half short side and their way out so far. */
export type WarpFrame = {
  zoom: [number, number, number, number];
  turn: [number, number];
  centre: [number, number];
  stretch: number;
  drift: [number, number];
  wobble: [number, number, number];
  petal: [number, number];
  ring: [number, number];
};

/** The speeds the Warp's two clocks run at (a second): its petals' turn (half a turn a second at ±1) and its rings'
 *  way out (two rings a second at ±1). */
export function warpSpeeds(k: Record<string, number>): { petal: number; ring: number } {
  return { petal: Math.PI * (k.petalTurn ?? 0), ring: 2 * TAU * (k.ringSpeed ?? 0.5) };
}

/** The Warp's two clocks (its petals' turn, its rings' way out), each a phase run on by its speed a frame at a time,
 *  so a change of speed (a cable, a macro, a morph) goes on from where it is instead of jumping. */
export class WarpClock {
  petal = 0;
  ring = 0;
  /** On by `dt` seconds at the speeds of the Warp `k` (the last of a loop's: the one whose counts are drawn). */
  run(k: Record<string, number>, dt: number) {
    const v = warpSpeeds(k);
    this.petal = (this.petal + v.petal * dt) % TAU;
    this.ring = (this.ring + v.ring * dt) % TAU;
  }
}

/** The frame of the Warps `chain` (each its knobs; its amount scales all of it but its counts and centre, the last
 *  Warp's), `dt` seconds long, on a target `aspect` wide to 1 tall, at `time` seconds (the wobble's), its clocks at
 *  `clock`. A second's worth at full: zoom half again (in or out), half a turn, a fifth of the picture across; by place
 *  the same rates. */
export function warpFrame(chain: Record<string, number>[], dt: number, aspect: number, time: number, clock: { petal: number; ring: number }): WarpFrame {
  let zoom = 0, edge = 0, petals = 0, rings = 0, turn = 0, turnEdge = 0, dx = 0, dy = 0, stretch = 0, wobble = 0;
  let size = 0.5, cx = 0, cy = 0, count = 4, ringCount = 3;
  for (const k of chain) {
    const a = k.amount ?? 0;
    zoom += a * (k.zoom ?? 0);
    edge += a * (k.zoomEdge ?? 0);
    petals += a * (k.petals ?? 0);
    rings += a * (k.rings ?? 0);
    turn += a * (k.turn ?? 0);
    turnEdge += a * (k.turnEdge ?? 0);
    dx += a * (k.driftX ?? 0);
    dy += a * (k.driftY ?? 0);
    stretch += a * (k.stretch ?? 0);
    wobble += a * (k.wobble ?? 0);
    size = k.wobbleSize ?? size;
    cx = k.centreX ?? cx;
    cy = k.centreY ?? cy;
    count = Math.max(0, k.petalCount ?? count);
    ringCount = Math.max(0, k.ringCount ?? ringCount);
  }
  const z = 0.4 * dt;
  return {
    zoom: [z * zoom, z * edge, z * petals, z * rings],
    turn: [Math.PI * turn * dt, Math.PI * turnEdge * dt],
    centre: [0.5 * aspect * cx, 0.5 * cy],
    stretch: Math.exp(0.2 * stretch * dt),
    drift: [0.2 * dx * dt, 0.2 * dy * dt],
    wobble: [0.02 * wobble * Math.min(1, dt * 30), 4 + 20 * size, time * 1.7],
    petal: [count, clock.petal],
    ring: [ringCount, clock.ring],
  };
}

/** The petals' shape round the centre at the angle `th` (−1..1): `count` of them, a count between two whole ones the
 *  two shapes mixed by its fraction (the next petal fades in; a whole count meets itself round the circle, so there is
 *  no seam), turned by `phase`. */
export function petalAt(th: number, count: number, phase: number): number {
  const n0 = Math.floor(count);
  const f = count - n0;
  return (1 - f) * Math.cos(n0 * th + phase) + f * Math.cos((n0 + 1) * th + phase);
}

/** Where the Warp reads the last picture from for the point `p` (centred, aspect-true: the short side −0.5..0.5), as
 *  WARP_GLSL's `warpFrom` does (the drift left to the caller, in the picture's 0..1), so Node can test it. */
export function warpFrom(p: [number, number], w: WarpFrame): [number, number] {
  let qx = p[0] - w.centre[0];
  let qy = p[1] - w.centre[1];
  const r = Math.hypot(qx, qy) * 2;
  const th = r > 1e-5 ? Math.atan2(qy, qx) : 0;
  const ring = Math.sin(TAU * w.ring[0] * r - w.ring[1]);
  const z = Math.exp(-(w.zoom[0] + w.zoom[1] * r + w.zoom[2] * petalAt(th, w.petal[0], w.petal[1]) + w.zoom[3] * ring));
  qx *= z;
  qy *= z;
  const a = w.turn[0] + w.turn[1] * r;
  const c = Math.cos(a);
  const s = Math.sin(a);
  [qx, qy] = [c * qx - s * qy, s * qx + c * qy];
  qx /= w.stretch;
  qy *= w.stretch;
  const x = qx + w.centre[0];
  const y = qy + w.centre[1];
  return [x + w.wobble[0] * Math.sin(y * w.wobble[1] + w.wobble[2]), y + w.wobble[0] * Math.cos(x * w.wobble[1] - w.wobble[2])];
}

/** The Warp in GLSL ES 3.00: its uniforms and `warpFrom(p)`, as `warpFrom` above (the drift left to the caller). No
 *  arrays (Mali's float[] miscompiles); atan only off the centre (atan(0, 0) is undefined on some GPUs). */
export const WARP_GLSL = `uniform vec4 uWarpZoom;
uniform vec2 uWarpTurn;
uniform vec2 uWarpCentre;
uniform float uWarpStretch;
uniform vec3 uWarpWobble;
uniform vec2 uWarpPetal;
uniform vec2 uWarpRing;
vec2 warpFrom(vec2 p) {
  vec2 q = p - uWarpCentre;
  float r = length(q) * 2.0;
  float th = r > 1e-5 ? atan(q.y, q.x) : 0.0;
  float n0 = floor(uWarpPetal.x);
  float f = uWarpPetal.x - n0;
  float petal = mix(cos(n0 * th + uWarpPetal.y), cos((n0 + 1.0) * th + uWarpPetal.y), f);
  float ring = sin(6.28318531 * uWarpRing.x * r - uWarpRing.y);
  q *= exp(-(uWarpZoom.x + uWarpZoom.y * r + uWarpZoom.z * petal + uWarpZoom.w * ring));
  float a = uWarpTurn.x + uWarpTurn.y * r;
  float c = cos(a), s = sin(a);
  q = vec2(c * q.x - s * q.y, s * q.x + c * q.y);
  q *= vec2(1.0 / uWarpStretch, uWarpStretch);
  p = q + uWarpCentre;
  return p + uWarpWobble.x * vec2(sin(p.y * uWarpWobble.y + uWarpWobble.z), cos(p.x * uWarpWobble.y - uWarpWobble.z));
}`;

/** WARP_GLSL's uniforms for the frame `w` (the drift is the caller's). */
export function warpUniforms(w: WarpFrame): Record<string, number | [number, number] | [number, number, number] | [number, number, number, number]> {
  return { uWarpZoom: w.zoom, uWarpTurn: w.turn, uWarpCentre: w.centre, uWarpStretch: w.stretch, uWarpWobble: w.wobble, uWarpPetal: w.petal, uWarpRing: w.ring };
}
