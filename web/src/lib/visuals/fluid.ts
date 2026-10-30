// A fluid the visuals' light can flow in (decision 0047): the flame's trails, instead of only fading where they were
// drawn, are carried along by a flow the music stirs, so they drift, curl and fold like smoke. The owner: "another way
// of having such dancing movement is through fluid dynamics", and in the next version "only if we can keep it
// development properly separate": this file is the fluid alone; the renderer calls it through the few methods below
// (decision 0047's seam) and the visuals turn the music into its forces.
//
// Stam's stable fluids on the GPU ("Real-Time Fluid Dynamics for Games", 2003; as in GPU Gems 1, ch. 38): a velocity
// field on a coarse grid of its own (a hundred cells or so across, whatever the screen), each step pushed by the
// forces, given back its small whirls (vorticity confinement, Fedkiw et al. 2001), made free of divergence (pressure by
// Jacobi iteration, then its gradient taken away) and carried along by itself; the light then carried along by it at
// the buffer's own size. In as few passes as that takes (eight a step): on a Mali tablet a pass cost more than its sums
// (seventeen were 5 ms a frame on a G52, the grid's size hardly counting; 2026-10-18). Velocities are in grid cells per second (`uvPerCell` turns them into texture coordinates).
//
// Written for the Mali GPUs Cratarr's devices have (2026-10-18): no array constructors in a shader; a value's bits only
// say whether it is a number, never what to answer with; no `isnan`; 16-bit float textures throughout (a G52 cannot
// filter 32-bit ones), drawn into with EXT_color_buffer_float.
//
// Each call changes the framebuffer bound, the program, the vertex array, the viewport, blending (off) and the
// textures bound to the first units: the renderer sets its own again after. `advect` uses the last `step`'s time, so
// a frame is `push`, `step`, then `advect`.

import { FIELD_GLSL, GIANT_SCALE, PLAIN, strengthsOf, type FieldShape } from './field';
import { toScreen } from './transform';

/** A push on the fluid: where (texture coordinates of the buffer, 0..1 from its bottom left), which way and how fast
 *  (texture coordinates per second, at strength 1), how far it reaches (in heights of the buffer) and how hard. */
export type Force = { x: number; y: number; dx: number; dy: number; radius: number; strength: number };

/** How the fluid moves now: `energy` 0..1 (how long its motion lasts: a calm passage settles within a second, a loud
 *  one flows on), `swirl` 0.. (how strongly its small whirls are kept). */
/** `wave`: the slow giant waves (65): how high (the Waves column's routes), which way they roll (radians) and where in
 *  their cycle (radians). */
/** `field`: a Field stirring it (the owner: the Field's bands shape the swirls, the fluid gives them inertia): its flow
 *  pushes the fluid everywhere, `amount` how hard (a cable's). */
/** Fire (0.9.18; the owner via fire-e, 2026-10-29): `lift` how hard the densest light rises by itself,
 *  as hot gas does (heights a second, each second; fire-e's 2); `open` how open the floor and top are (0 sealed, as ever;
 *  1 open: air comes in below and goes out above, the sides still walls); `cooling` how fast the light burns out as it
 *  climbs, from 40 % of the height up and along the side walls above the bed (its share lost a second; fire-e's 3).
 *  None of them, nothing changes. */
export type Flow = { energy: number; swirl: number; wave?: Wave; field?: { scale: number; phase: number; waves?: readonly number[]; shape?: FieldShape; amount: number }; lift?: number; open?: number; cooling?: number };
/** What lifts the fluid: the light it last carried, and the scale that puts the light's density on the display's
 *  (nodes/show.ts densityScale), so Lift means the same whatever the points, the buffer's size or the keep. */
export type Heat = { light: WebGLTexture; k: number };
/** The slow giant waves: how high and where in their roll (`angle` no longer counts: the Field's sum has every way). */
export type Wave = { amount: number; angle: number; phase: number };

/** The giant waves (field.ts GIANT_SCALE, the Field's sum): how hard they push at amount 1 (as the sine before them, about
 *  a third of the screen's height a second, each second), and how far they bend the picture where nothing flows (its
 *  flow's seconds: about a fortieth of the screen at their fullest). */
export const WAVE_PUSH = 1.4;
export const WAVE_BEND = 0.1;

/** The light the renderer keeps: its colour and density, and its two band-share buffers. */
export type Light = { acc: WebGLTexture; bandsA: WebGLTexture; bandsB: WebGLTexture };

/** The most forces taken a step; more are dropped (the strongest kept). */
export const MOST_FORCES = 16;
/** Jacobi iterations a step: enough for smoke that looks right, few enough for a tablet (each is a pass). */
const ITERATIONS = 6;
/** An open box's (fire-e's): its pressure settles against an edge, not a drifting constant, so it takes more. */
const ITERATIONS_OPEN = 20;
/** The fastest the fluid moves (cells a second): beyond it, a bad step could blow up. */
const FASTEST = 400;
/** What is left of the last step's pressure (a warm start for the iteration). */
const PRESSURE_KEPT = 0.8;
/** An open box's warm start: kept whole. */
const PRESSURE_KEPT_OPEN = 1;
/** Lift's and Cooling's most (heights a second each second; the share lost a second, as a rate). */
export const LIFT_MOST = 4;
export const COOLING_MOST = 6;
/** How hard a Field stirs at amount 1: its flow's speed reached in about a second, before the fluid's own losses. */
const FIELD_STIR = 1;
/** Vorticity confinement at swirl 1 (Dobryakov's WebGL fluid uses 30 at this scale). */
const SWIRL = 30;

const VERTEX = `#version 300 es
precision highp float;
out vec2 vUv;
// One triangle over the whole target: no buffers, only the vertex's number.
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vUv = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

// Whether a value is not a number or infinite, from its bits (which no compiler's fast arithmetic assumes away).
const BAD = `bool bad(float x) { return (floatBitsToUint(x) & 0x7f800000u) == 0x7f800000u; }`;

const HEAD = `#version 300 es
precision highp float;
precision highp sampler2D;
in vec2 vUv;
${BAD}
`;

// The forces, and the small whirls given back (vorticity confinement): one pass, the curl worked out where it is
// needed (a pass of its own costs a Mali GPU more than the sums: each one is a flush of its tiles).
const FORCE = `${HEAD}
uniform sampler2D uVelocity;
uniform vec2 uTexel;
uniform float uAspect;
uniform vec2 uGrid;
uniform int uCount;
uniform vec4 uAt[${MOST_FORCES}];
uniform vec2 uWay[${MOST_FORCES}];
uniform float uSwirl;
uniform float uDt;
uniform float uWave;
uniform float uField;
uniform float uFieldScale;
uniform float uFieldPhase;
uniform vec3 uFieldA;
uniform vec3 uFieldB;
uniform vec2 uFieldShape;
uniform vec2 uK;
uniform sampler2D uHeat;
uniform float uLift;
uniform float uHeatK;
out vec4 o;
${FIELD_GLSL}
float curlAt(vec2 uv) {
  float l = texture(uVelocity, uv - vec2(uTexel.x, 0.0)).y;
  float r = texture(uVelocity, uv + vec2(uTexel.x, 0.0)).y;
  float b = texture(uVelocity, uv - vec2(0.0, uTexel.y)).x;
  float t = texture(uVelocity, uv + vec2(0.0, uTexel.y)).x;
  return 0.5 * (r - l - t + b);
}
void main() {
  vec2 v = texture(uVelocity, vUv).xy;
  float l = curlAt(vUv - vec2(uTexel.x, 0.0));
  float r = curlAt(vUv + vec2(uTexel.x, 0.0));
  float b = curlAt(vUv - vec2(0.0, uTexel.y));
  float t = curlAt(vUv + vec2(0.0, uTexel.y));
  float c = curlAt(vUv);
  vec2 f = 0.5 * vec2(abs(t) - abs(b), abs(r) - abs(l));
  f /= length(f) + 0.0001;
  v += f * uSwirl * c * vec2(1.0, -1.0) * uDt;
  // A Field stirring it, and the slow giant waves (one source: the Field cabled in, else the Field's own sum at the
  // giants' scale): its flow (the frame's units a second, centred, the short side -1..1) as a push everywhere, in cells
  // a second each second (free of divergence already: the pressure hardly has to touch it).
  float push = uField + uWave;
  if (push != 0.0) {
    vec2 p = (vUv * 2.0 - 1.0) / uK;
    v += fieldAt(p, uFieldScale, uFieldPhase, uFieldA, uFieldB, uFieldShape) * uK * 0.5 * uGrid * push * uDt;
  }
  // Lift: the light the fluid last carried is heat, and pushes its cell up (fire-e's buoyancy).
  if (uLift > 0.0) {
    // At the light's own level: its coarser ones are made for the smoothing, stale otherwise (as the light pass reads).
    float heat = 1.0 - exp(-0.25 * textureLod(uHeat, vUv, 0.0).a * uHeatK);
    v.y += uLift * heat * uGrid.y * uDt;
  }
  for (int i = 0; i < ${MOST_FORCES}; i++) {
    if (i >= uCount) break;
    vec2 d = vUv - uAt[i].xy;
    d.x *= uAspect;
    float radius = max(uAt[i].z, 0.001);
    v += uWay[i] * uGrid * uAt[i].w * exp(-dot(d, d) / (radius * radius));
  }
  o = bad(v.x + v.y) ? vec4(0.0) : vec4(clamp(v, -${FASTEST}.0, ${FASTEST}.0), 0.0, 1.0);
}`;

// One Jacobi iteration of the pressure, the velocity's divergence worked out in it (the edges are walls: what flows
// into one comes back); `uWarm` scales the pressure it starts from (the last step's, a little of it kept). `uOpen`: the
// floor and top open by so much (no reflection there, and the pressure 0 beyond them: Dirichlet); the sides stay walls.
const JACOBI = `${HEAD}
uniform sampler2D uPressure;
uniform sampler2D uVelocity;
uniform vec2 uTexel;
uniform float uWarm;
uniform float uOpen;
out vec4 o;
void main() {
  vec2 c = texture(uVelocity, vUv).xy;
  float vl = texture(uVelocity, vUv - vec2(uTexel.x, 0.0)).x;
  float vr = texture(uVelocity, vUv + vec2(uTexel.x, 0.0)).x;
  float vb = texture(uVelocity, vUv - vec2(0.0, uTexel.y)).y;
  float vt = texture(uVelocity, vUv + vec2(0.0, uTexel.y)).y;
  if (vUv.x - uTexel.x < 0.0) vl = -c.x;
  if (vUv.x + uTexel.x > 1.0) vr = -c.x;
  bool below = vUv.y - uTexel.y < 0.0;
  bool above = vUv.y + uTexel.y > 1.0;
  if (below) vb = mix(-c.y, vb, uOpen);
  if (above) vt = mix(-c.y, vt, uOpen);
  float d = 0.5 * (vr - vl + vt - vb);
  float l = texture(uPressure, vUv - vec2(uTexel.x, 0.0)).x;
  float r = texture(uPressure, vUv + vec2(uTexel.x, 0.0)).x;
  float b = texture(uPressure, vUv - vec2(0.0, uTexel.y)).x;
  float t = texture(uPressure, vUv + vec2(0.0, uTexel.y)).x;
  if (below) b *= 1.0 - uOpen;
  if (above) t *= 1.0 - uOpen;
  o = vec4(((l + r + b + t) * uWarm - d) * 0.25, 0.0, 0.0, 1.0);
}`;

// The pressure's gradient taken away (the flow free of divergence), and the result carried along by itself, a little
// of it lost: one pass, the free velocity worked out where it is sampled. Open (`uOpen`): the pressure 0 beyond the floor
// and top, as the iteration has it, and the gradient at the half step the divergence is (sealed, the full one, as ever:
// it made up for a solve kept loose); nothing flows down through the floor or the top rows; the sideways motion damped
// (fire is mostly vertical: else one roll sweeps it to a side) and the flow calmed along the side walls (without it,
// "chaos on the edges and weak in the middle": the owner, of fire-e).
const CARRY = `${HEAD}
uniform sampler2D uVelocity;
uniform sampler2D uPressure;
uniform vec2 uTexel;
uniform float uDt;
uniform float uKeep;
uniform float uOpen;
out vec4 o;
vec2 free(vec2 uv) {
  float l = texture(uPressure, uv - vec2(uTexel.x, 0.0)).x;
  float r = texture(uPressure, uv + vec2(uTexel.x, 0.0)).x;
  float b = texture(uPressure, uv - vec2(0.0, uTexel.y)).x;
  float t = texture(uPressure, uv + vec2(0.0, uTexel.y)).x;
  if (uv.y - uTexel.y < 0.0) b *= 1.0 - uOpen;
  if (uv.y + uTexel.y > 1.0) t *= 1.0 - uOpen;
  return texture(uVelocity, uv).xy - mix(1.0, 0.5, uOpen) * vec2(r - l, t - b);
}
void main() {
  vec2 back = vUv - uDt * free(vUv) * uTexel;
  vec2 v = free(back) * uKeep;
  if (uOpen > 0.0) {
    if (vUv.y < uTexel.y || vUv.y > 1.0 - uTexel.y) v.y = mix(v.y, max(v.y, 0.0), uOpen);
    v.x *= mix(1.0, exp(-2.0 * uDt), uOpen);
    float side = max(1.0 - smoothstep(0.0, 0.1, vUv.x), smoothstep(0.9, 1.0, vUv.x));
    v *= mix(1.0, exp(-6.0 * uDt), side * uOpen);
  }
  o = bad(v.x + v.y) ? vec4(0.0) : vec4(clamp(v, -${FASTEST}.0, ${FASTEST}.0), 0.0, 1.0);
}`;

// The light carried along by the velocity, at the buffer's own size, and as much of it kept as the fade would have. A
// sample that is not a number (anywhere in it) is nothing: carried, it would spread to its neighbours each frame.
/** The light pass, with the loop slot's shape (effects.ts `loopFunction`: `fx_loop`) on where it reads back. */
const lightSource = (loop: string) => `${HEAD}
uniform sampler2D uVelocity;
uniform sampler2D uAcc;
uniform sampler2D uBandsA;
uniform sampler2D uBandsB;
uniform vec2 uCell;
uniform float uDt;
uniform float uKeep;
uniform float uZoom;
uniform float uLoopA;
uniform vec4 uLoopTime;
uniform float uLoopFrames;
uniform float uLoopAspect;
uniform vec2 uLoopK;
uniform float uFacets;
uniform float uLattice;
uniform float uOpen;
uniform float uCool;
layout(location = 0) out vec4 o;
layout(location = 1) out vec4 bandA;
layout(location = 2) out vec4 bandB;
vec4 kept(vec4 v) { return bad(dot(v, vec4(1.0))) ? vec4(0.0) : v * uKeep; }
// Cooling (uCool: its rate times the step): the light burns out ever faster from 40 % of the height up, and along the
// side walls above the bed, so the flames end where fire does, and never gather under the top or climb a wall.
float cooled() {
  if (uCool <= 0.0) return 1.0;
  float side = smoothstep(0.15, 0.3, vUv.y) * max(1.0 - smoothstep(0.0, 0.07, vUv.x), smoothstep(0.93, 1.0, vUv.x));
  return exp(-uCool * max(smoothstep(0.4, 0.95, vUv.y), side));
}
${loop}
// Facets: where the light is read back, pulled toward its cell's middle (the fluid's cells, square on the screen; or
// hexagons a cell across), so each cell carries one light: softly at a little, as hard tiles at all of it.
vec2 facet(vec2 uv) {
  if (uFacets <= 0.0) return uv;
  vec2 c = uv / uCell;
  vec2 centre;
  if (uLattice < 0.5) centre = floor(c) + 0.5;
  else {
    const float S = 0.57735027;
    float q = (0.57735027 * c.x - c.y / 3.0) / S;
    float r = (2.0 / 3.0 * c.y) / S;
    vec3 cube = vec3(q, r, -q - r);
    vec3 rc = round(cube);
    vec3 d = abs(rc - cube);
    if (d.x > d.y && d.x > d.z) rc.x = -rc.y - rc.z;
    else if (d.y > d.z) rc.y = -rc.x - rc.z;
    centre = S * vec2(1.7320508 * rc.x + 0.8660254 * rc.y, 1.5 * rc.y);
  }
  return mix(uv, centre * uCell, uFacets);
}
// Read at the light's own level, never a coarser one: where the flow squeezes, an implicit level blends in some of
// the coarser levels (made for a wide smoothing, stale otherwise), stepping at every cell's edge, and the steps pile up
// frame on frame into blocks (2026-10-18: Mali takes 7% of the next level for a 5% squeeze; desktops round it away).
void main() {
  // The tunnel (zoom.ts): read nearer the middle, so what was drawn is drawn larger.
  vec2 at = 0.5 + (vUv - 0.5) * uZoom;
  at = fx_loop(at, uLoopA, uLoopTime, uLoopFrames, uLoopAspect, uLoopK);
  vec2 back = facet(at - uDt * texture(uVelocity, at).xy * uCell);
  // Air drawn in through an open floor arrives dark: what is read from below it is nothing (clamped, it would be the
  // bottom row pulled up the screen in a column).
  float k = (back.y < 0.0 ? 1.0 - uOpen : 1.0) * cooled();
  o = kept(textureLod(uAcc, back, 0.0)) * k;
  bandA = kept(textureLod(uBandsA, back, 0.0)) * k;
  bandB = kept(textureLod(uBandsB, back, 0.0)) * k;
}`;

// A picture carried along by the velocity (the picture loop's Fluid, decision 0056: `carry`), nothing of it lost: what
// fades is the loop's Keep's. At no time (`uDt` 0) it reads back exactly where it draws: a plain copy.
const PICTURE = `${HEAD}
uniform sampler2D uVelocity;
uniform sampler2D uPicture;
uniform vec2 uCell;
uniform float uDt;
out vec4 o;
void main() {
  vec2 back = vUv - uDt * texture(uVelocity, vUv).xy * uCell;
  vec4 v = textureLod(uPicture, back, 0.0);
  o = bad(dot(v, vec4(1.0))) ? vec4(0.0) : v;
}`;

type Target = { texture: WebGLTexture; fbo: WebGLFramebuffer };

/** A step's Fire as its passes take it (pure): each within its range (none where not a number), Lift only where there is
 *  heat to read, and the pressure's iterations and warm start from sealed's to open's as the floor and top open. With
 *  none of them, the fluid as it always was. */
export function fireOf(flow: Pick<Flow, 'lift' | 'open' | 'cooling'>, heat?: Heat | null) {
  const within = (v: number | undefined, hi: number) => (v !== undefined && Number.isFinite(v) ? Math.min(hi, Math.max(0, v)) : 0);
  const open = within(flow.open, 1);
  return {
    lift: heat ? within(flow.lift, LIFT_MOST) : 0,
    open,
    cooling: within(flow.cooling, COOLING_MOST),
    iterations: Math.round(ITERATIONS + (ITERATIONS_OPEN - ITERATIONS) * open),
    warm: PRESSURE_KEPT + (PRESSURE_KEPT_OPEN - PRESSURE_KEPT) * open,
  };
}

/** `fx_flow` in the light pass, for a loop effect that reads the flow (effects.ts Stream): the velocity where p is, in
 *  p's units a second (centred, aspect-true), as the picture slot's own. */
export const FLOW_IN_LIGHT = `vec2 fx_flow(vec2 p) {
  vec2 uv = vec2(p.x / uLoopAspect, p.y) * 0.5 + 0.5;
  return textureLod(uVelocity, uv, 0.0).xy * uCell * 2.0 * vec2(uLoopAspect, 1.0);
}`;

/** The loop slot's shape for the light pass (effects.ts `loopFunction`): its program's key and GLSL, its amount, its
 *  own time and what that moved this frame, this frame's length in sixtieths of a second, its knobs. */
export type Loop = { key: string; glsl: string; amount: number; time: readonly [number, number, number, number]; frames: number; knobs?: readonly [number, number] };
const NO_LOOP = `vec2 fx_loop(vec2 uv, float a, vec4 time, float frames, float aspect, vec2 k) { return uv; }`;

/** The fluid's grid for a buffer of `width` x `height`: about `cells` cells, as the buffer is shaped. */
export function gridFor(width: number, height: number, cells = 96 * 54): [number, number] {
  const aspect = Math.max(0.1, width / Math.max(1, height));
  const h = Math.max(8, Math.round(Math.sqrt(cells / aspect)));
  return [Math.max(8, Math.round(h * aspect)), h];
}

export class Fluid {
  private gl: WebGL2RenderingContext;
  private empty: WebGLVertexArrayObject;
  private programs = new Map<string, WebGLProgram>();
  private locations = new Map<WebGLProgram, Map<string, WebGLUniformLocation | null>>();
  private velocity2: [Target, Target] | null = null;
  private pressure2: [Target, Target] | null = null;
  private pending: Force[] = [];
  private lastDt = 0;
  /** The last step's open floor and top, and its cooling, for the light pass after it. */
  private lastOpen = 0;
  private lastCooling = 0;
  width = 0;
  height = 0;

  /** Whether `gl` can hold the fluid: it draws into 16-bit float textures. */
  static supported(gl: WebGL2RenderingContext): boolean {
    return !!gl.getExtension('EXT_color_buffer_float');
  }

  /** The fluid on a grid of `gridW` x `gridH` cells (`gridFor`); throws where the device cannot hold it. */
  constructor(gl: WebGL2RenderingContext, gridW: number, gridH: number) {
    if (!Fluid.supported(gl)) throw new Error('This device cannot draw into float textures.');
    this.gl = gl;
    this.empty = gl.createVertexArray()!;
    const programs = { FORCE, JACOBI, CARRY, PICTURE, LIGHT: lightSource(NO_LOOP) };
    for (const [name, source] of Object.entries(programs)) this.programs.set(name, program(gl, VERTEX, source));
    this.resize(gridW, gridH);
  }

  /** The velocity now, a texture of the grid (RG: cells a second); `uvPerCell` turns it into texture coordinates. */
  get velocity(): WebGLTexture {
    return this.velocity2![0].texture;
  }

  /** The velocity read back into `out` (RGBA floats a cell, rows from the bottom, `width` x `height` of them): a
   *  synchronous read, for what needs the flow on the CPU (a Grid drifting on it). Leaves its framebuffer bound. */
  read(out: Float32Array): boolean {
    const t = this.velocity2?.[0];
    if (!t || out.length < this.width * this.height * 4) return false;
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, t.fbo);
    gl.readPixels(0, 0, this.width, this.height, gl.RGBA, gl.FLOAT, out);
    return true;
  }

  /** Texture coordinates of the buffer per cell of the grid (x, y). */
  get uvPerCell(): [number, number] {
    return [1 / this.width, 1 / this.height];
  }

  /** The grid made anew at another size (still). */
  resize(gridW: number, gridH: number) {
    gridW = Math.max(8, Math.round(gridW));
    gridH = Math.max(8, Math.round(gridH));
    if (gridW === this.width && gridH === this.height) return;
    this.freeGrid();
    this.width = gridW;
    this.height = gridH;
    const gl = this.gl;
    this.velocity2 = [this.target(gl.RG16F, gl.RG), this.target(gl.RG16F, gl.RG)];
    this.pressure2 = [this.target(gl.R16F, gl.RED), this.target(gl.R16F, gl.RED)];
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** Forces for the next step (added to any not taken yet; the strongest `MOST_FORCES` are). */
  push(forces: Force[]) {
    this.pending.push(...forces.filter((f) => [f.x, f.y, f.dx, f.dy, f.radius, f.strength].every(Number.isFinite)));
  }

  /** One step of the flow, `dt` seconds on (at most a twentieth: a long pause does not throw it): two passes and the
   *  pressure's iterations, on the grid; `heat` what Lift reads (none: no lift). */
  step(dt: number, flow: Flow, heat?: Heat | null) {
    const gl = this.gl;
    dt = Math.min(0.05, Math.max(0, dt));
    this.lastDt = dt;
    const fire = fireOf(flow, heat);
    this.lastOpen = fire.open;
    this.lastCooling = fire.cooling;
    gl.disable(gl.BLEND);
    gl.bindVertexArray(this.empty);
    gl.viewport(0, 0, this.width, this.height);
    const texel = [1 / this.width, 1 / this.height] as const;
    const v = this.velocity2!;
    const p = this.pressure2!;

    const forces = this.pending.sort((a, b) => Math.abs(b.strength) - Math.abs(a.strength)).slice(0, MOST_FORCES);
    this.pending = [];
    const at = new Float32Array(MOST_FORCES * 4);
    const way = new Float32Array(MOST_FORCES * 2);
    forces.forEach((f, i) => {
      at.set([f.x, f.y, f.radius, f.strength], i * 4);
      way.set([f.dx, f.dy], i * 2);
    });
    this.draw('FORCE', v[1], { uVelocity: v[0].texture, uHeat: heat?.light ?? v[0].texture }, (u) => {
      gl.uniform2f(u('uTexel'), ...texel);
      gl.uniform1f(u('uAspect'), this.width / this.height);
      gl.uniform2f(u('uGrid'), this.width, this.height);
      gl.uniform1i(u('uCount'), forces.length);
      gl.uniform4fv(u('uAt'), at);
      gl.uniform2fv(u('uWay'), way);
      gl.uniform1f(u('uSwirl'), SWIRL * Math.max(0, flow.swirl));
      gl.uniform1f(u('uDt'), dt);
      const w = flow.wave;
      const f = flow.field;
      const shape = f?.shape ?? PLAIN;
      const fw = strengthsOf(f?.waves, shape);
      gl.uniform2f(u('uFieldShape'), shape.range, shape.variety);
      gl.uniform1f(u('uWave'), w ? WAVE_PUSH * w.amount : 0);
      gl.uniform1f(u('uField'), f ? FIELD_STIR * Math.max(0, f.amount) : 0);
      gl.uniform1f(u('uFieldScale'), f?.scale ?? GIANT_SCALE);
      gl.uniform1f(u('uFieldPhase'), f?.phase ?? w?.phase ?? 0);
      gl.uniform3f(u('uFieldA'), fw[0], fw[1], fw[2]);
      gl.uniform3f(u('uFieldB'), fw[3], fw[4], fw[5]);
      gl.uniform2f(u('uK'), ...toScreen(this.width / this.height));
      gl.uniform1f(u('uLift'), fire.lift);
      gl.uniform1f(u('uHeatK'), heat?.k ?? 0);
    });
    v.reverse();

    for (let i = 0; i < fire.iterations; i++) {
      this.draw('JACOBI', p[1], { uPressure: p[0].texture, uVelocity: v[0].texture }, (u) => {
        gl.uniform2f(u('uTexel'), ...texel);
        gl.uniform1f(u('uWarm'), i === 0 ? fire.warm : 1);
        gl.uniform1f(u('uOpen'), fire.open);
      });
      p.reverse();
    }

    // How long the motion lasts: about three seconds at full energy, a third of one at none.
    const energy = Math.min(1, Math.max(0, flow.energy));
    const keep = Math.exp(-dt * (0.3 + 2.7 * (1 - energy)));
    this.draw('CARRY', v[1], { uVelocity: v[0].texture, uPressure: p[0].texture }, (u) => {
      gl.uniform2f(u('uTexel'), ...texel);
      gl.uniform1f(u('uDt'), dt);
      gl.uniform1f(u('uKeep'), keep);
      gl.uniform1f(u('uOpen'), fire.open);
    });
    v.reverse();
  }

  /** The light carried along by the flow into `dst` (a framebuffer of three targets as `light`'s, `width` x `height`),
   *  `keep` of it kept (as the fade keeps it), in place of the renderer's fade. Leaves blending off and `dst` bound. */
  advect(light: Light, dst: WebGLFramebuffer, keep: number, width: number, height: number, zoom = 1, loop?: Loop, facets?: { amount: number; hex: boolean }) {
    const gl = this.gl;
    gl.disable(gl.BLEND);
    gl.bindVertexArray(this.empty);
    gl.bindFramebuffer(gl.FRAMEBUFFER, dst);
    gl.drawBuffers([gl.COLOR_ATTACHMENT0, gl.COLOR_ATTACHMENT1, gl.COLOR_ATTACHMENT2]);
    gl.viewport(0, 0, width, height);
    const name = loop ? `LIGHT:${loop.key}` : 'LIGHT';
    if (loop && !this.programs.has(name)) {
      // One that does not compile on this device: the light pass without it, rather than no frame.
      try {
        this.programs.set(name, program(gl, VERTEX, lightSource(loop.glsl)));
      } catch (e) {
        console.warn('Visuals: a loop effect did not compile here', loop.key, e);
        this.programs.set(name, this.programs.get('LIGHT')!);
      }
    }
    const prog = this.use(name);
    const u = (name: string) => this.location(prog, name);
    this.bind(prog, { uVelocity: this.velocity, uAcc: light.acc, uBandsA: light.bandsA, uBandsB: light.bandsB });
    gl.uniform2f(u('uCell'), ...this.uvPerCell);
    gl.uniform1f(u('uDt'), this.lastDt);
    gl.uniform1f(u('uKeep'), keep);
    gl.uniform1f(u('uZoom'), zoom);
    gl.uniform1f(u('uLoopA'), loop?.amount ?? 0);
    gl.uniform4f(u('uLoopTime'), ...(loop?.time ?? ([0, 0, 0, 0] as const)));
    gl.uniform1f(u('uLoopFrames'), loop?.frames ?? 1);
    gl.uniform1f(u('uLoopAspect'), width / height);
    gl.uniform2f(u('uLoopK'), loop?.knobs?.[0] ?? 0.5, loop?.knobs?.[1] ?? 0.5);
    gl.uniform1f(u('uFacets'), facets?.amount ?? 0);
    gl.uniform1f(u('uLattice'), facets?.hex ? 1 : 0);
    gl.uniform1f(u('uOpen'), this.lastOpen);
    gl.uniform1f(u('uCool'), this.lastCooling * this.lastDt);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  /** A picture (`input`, `width` x `height`) carried along by the flow into `out` (null: the screen), `amount` of the
   *  last step's way (0: a plain copy). The picture loop's Fluid (decision 0056); `advect` is the flame's own. Leaves
   *  blending off and `out` bound. */
  carry(input: WebGLTexture, out: WebGLFramebuffer | null, width: number, height: number, amount: number) {
    const gl = this.gl;
    gl.disable(gl.BLEND);
    gl.bindVertexArray(this.empty);
    gl.bindFramebuffer(gl.FRAMEBUFFER, out);
    if (out) gl.drawBuffers([gl.COLOR_ATTACHMENT0]);
    gl.viewport(0, 0, width, height);
    const prog = this.use('PICTURE');
    const u = (name: string) => this.location(prog, name);
    this.bind(prog, { uVelocity: this.velocity, uPicture: input });
    gl.uniform2f(u('uCell'), ...this.uvPerCell);
    gl.uniform1f(u('uDt'), this.lastDt * Math.max(0, amount));
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  dispose() {
    const gl = this.gl;
    this.freeGrid();
    for (const p of this.programs.values()) gl.deleteProgram(p);
    this.programs.clear();
    gl.deleteVertexArray(this.empty);
  }

  private freeGrid() {
    const gl = this.gl;
    for (const t of [...(this.velocity2 ?? []), ...(this.pressure2 ?? [])]) {
      if (!t) continue;
      gl.deleteFramebuffer(t.fbo);
      gl.deleteTexture(t.texture);
    }
    this.velocity2 = this.pressure2 = null;
  }

  private target(internal: number, format: number): Target {
    const gl = this.gl;
    const texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texImage2D(gl.TEXTURE_2D, 0, internal, this.width, this.height, 0, format, gl.HALF_FLOAT, null);
    const fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) throw new Error('This device cannot draw into the fluid’s textures.');
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    return { texture, fbo };
  }

  private use(name: string): WebGLProgram {
    const p = this.programs.get(name)!;
    this.gl.useProgram(p);
    return p;
  }

  private location(p: WebGLProgram, name: string): WebGLUniformLocation | null {
    let m = this.locations.get(p);
    if (!m) this.locations.set(p, (m = new Map()));
    if (!m.has(name)) m.set(name, this.gl.getUniformLocation(p, name));
    return m.get(name) ?? null;
  }

  private bind(p: WebGLProgram, textures: Record<string, WebGLTexture>) {
    const gl = this.gl;
    Object.entries(textures).forEach(([name, texture], unit) => {
      gl.activeTexture(gl.TEXTURE0 + unit);
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.uniform1i(this.location(p, name), unit);
    });
  }

  private draw(name: string, into: Target, textures: Record<string, WebGLTexture>, set: (u: (n: string) => WebGLUniformLocation | null) => void) {
    const gl = this.gl;
    const p = this.use(name);
    gl.bindFramebuffer(gl.FRAMEBUFFER, into.fbo);
    this.bind(p, textures);
    set((n) => this.location(p, n));
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }
}

function program(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const shader = (type: number, src: string) => {
    const s = gl.createShader(type)!;
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'A fluid shader did not compile.');
    return s;
  };
  const p = gl.createProgram()!;
  const [v, f] = [shader(gl.VERTEX_SHADER, vs), shader(gl.FRAGMENT_SHADER, fs)];
  gl.attachShader(p, v);
  gl.attachShader(p, f);
  gl.linkProgram(p);
  gl.deleteShader(v);
  gl.deleteShader(f);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? 'A fluid program did not link.');
  return p;
}
