// Field (the owner's evolving flow field): a 2D flow that changes smoothly over time, the curl of a stream function made
// of a few travelling waves (each its own direction, a little off the others, its own size and its own pace), so it has
// no sources or sinks: what it carries neither bunches up nor thins out. The same sum here (for points it moves) and in
// GLSL (for Streamlines), in the frame Grid, Instance and Fold use: centred, y up, the short side −1..1. Each wave its own
// strength (Waves, six channels: a band's cable gives each its band, the bass the biggest swirls, the treble the
// finest). Its shape (the owner: big curves for large vortexes, small for small ones): Range spreads the waves' sizes
// (the biggest Scale × LONGEST, each next smaller by the same ratio, the smallest Range times smaller), Roughness sets
// how strong the small ones are against the big (1: alike), Variety lets the small ones come and go across the frame
// with a slow giant mask (its stream function masked: still free of divergence). Pure.

/** The waves, the biggest first: direction (radians), size at the plain shape (times Scale), pace (times Speed),
 *  phase. Their sizes come from Range (`sizesOf`); the second column is what the plain Range gives, near enough. */
export const WAVES: readonly (readonly [number, number, number, number])[] = [
  [4.8, 1.31, 0.8, 3.1],
  [5.7, 1.13, 1.1, 2.3],
  [0.0, 1.0, 1.0, 0.0],
  [3.6, 0.89, -0.6, 5.2],
  [2.4, 0.73, -1.3, 1.7],
  [1.1, 0.57, 1.7, 4.4],
];

/** The biggest wave's size, times Scale. */
const LONGEST = 1.31;

/** A Field's shape: Range, Roughness, Variety. */
export type FieldShape = { range: number; roughness: number; variety: number };
/** The shape the Field had before it had one (its six sizes 2.3 times apart, all as strong, none coming and going). */
export const PLAIN: FieldShape = { range: 2.3, roughness: 1, variety: 0 };

/** Each wave's size against the biggest (1 down to 1/Range, the same ratio each step). */
const ratiosOf = (range: number) => WAVES.map((_, i) => Math.max(1, range) ** (-i / (WAVES.length - 1)));

/** Each wave's strength: its Waves (1 where none), times its Roughness weight (smaller ones weaker below 1, stronger
 *  above). What the shaders take as `wa`, `wb`. */
export function strengthsOf(waves: readonly number[] | undefined, shape: FieldShape = PLAIN): number[] {
  return ratiosOf(shape.range).map((r, i) => (waves?.[i] ?? 1) * r ** (1 - shape.roughness));
}

/** The slow giant mask Variety reads (about half the frame a patch, drifting with the Field's time), and its gradient. */
function maskAt(x: number, y: number, phase: number): [number, number, number] {
  const a = 2.1 * x + 0.31 * phase;
  const b = 1.7 * y + 1.3 - 0.23 * phase;
  return [0.5 + 0.5 * Math.sin(a) * Math.sin(b), 1.05 * Math.cos(a) * Math.sin(b), 0.85 * Math.sin(a) * Math.cos(b)];
}

/** The flow at (x, y) for a field of `scale` (its swirls' size, in the frame's units) at `phase` (its time, run on at
 *  its Speed), each wave at its strength in `waves` (1 where none), of `shape`: the velocity, about 1 at its strongest. */
export function flowAt(x: number, y: number, scale: number, phase: number, waves?: readonly number[], shape: FieldShape = PLAIN): [number, number] {
  const ratios = ratiosOf(shape.range);
  const strengths = strengthsOf(waves, shape);
  const [m, mx, my] = shape.variety ? maskAt(x, y, phase) : [1, 0, 0];
  let u = 0;
  let v = 0;
  for (let i = 0; i < WAVES.length; i++) {
    const [dir, , pace, offset] = WAVES[i];
    const k = (2 * Math.PI) / Math.max(0.01, scale * LONGEST * ratios[i]);
    const kx = k * Math.cos(dir);
    const ky = k * Math.sin(dir);
    const th = kx * x + ky * y + pace * phase + offset;
    const c = (strengths[i] * Math.cos(th)) / k;
    // Its stream function masked: the smaller the wave, the more the mask takes it away where it is low.
    const t = (shape.variety * i) / (WAVES.length - 1);
    const g = 1 - t * (1 - m);
    const psi = (strengths[i] * Math.sin(th)) / k;
    u += g * c * ky + psi * t * my;
    v -= g * c * kx + psi * t * mx;
  }
  return [(u / WAVES.length) * 2, (v / WAVES.length) * 2];
}

/** The same in GLSL: `vec2 fieldAt(vec2 p, float scale, float phase, vec3 wa, vec3 wb, vec2 shape)`, the strengths
 *  (`strengthsOf`) in two vec3s (not a float[]: Mali), `shape` Range and Variety. */
export const FIELD_GLSL = `vec2 fieldAt(vec2 p, float scale, float phase, vec3 wa, vec3 wb, vec2 shape) {
  vec2 f = vec2(0.0);
  float q = pow(max(1.0, shape.x), ${(-1 / (WAVES.length - 1)).toFixed(6)});
  float size = scale * ${LONGEST.toFixed(6)};
  float ma = 2.1 * p.x + 0.31 * phase;
  float mb = 1.7 * p.y + 1.3 - 0.23 * phase;
  float m = shape.y > 0.0 ? 0.5 + 0.5 * sin(ma) * sin(mb) : 1.0;
  vec2 mg = shape.y > 0.0 ? vec2(1.05 * cos(ma) * sin(mb), 0.85 * sin(ma) * cos(mb)) : vec2(0.0);
${WAVES.map(([dir, , pace, offset], i) => {
  const c = Math.cos(dir).toFixed(6);
  const s = Math.sin(dir).toFixed(6);
  const t = (i / (WAVES.length - 1)).toFixed(6);
  return `  { float k = 6.28318530718 / max(0.01, size); float th = k * (${c} * p.x + ${s} * p.y) + ${pace.toFixed(6)} * phase + ${offset.toFixed(6)}; float w = ${i < 3 ? 'wa' : 'wb'}.${'xyz'[i % 3]}; float t = shape.y * ${t}; float c = w * cos(th) / k; float psi = w * sin(th) / k; f += (1.0 - t * (1.0 - m)) * c * vec2(k * ${s}, -k * ${c}) + psi * t * vec2(mg.y, -mg.x); size *= q; }`;
}).join('\n')}
  return f / ${WAVES.length.toFixed(1)} * 2.0;
}`;

/** The slow giant waves (the Fluid's push, Show's and Bend's bend where nothing flows): the Field's own sum at this
 *  scale, every wave whole and plain (the owner: one source of waves), rolling on at their phase. */
export const GIANT_SCALE = 2.5;

/** In GLSL, after FIELD_GLSL: `kOf(aspect)` (transform.ts's toScreen) and `giantAt(uv, k, phase)`, the giant waves'
 *  flow at `uv` in texture coordinates a second. */
export const GIANT_GLSL = `vec2 kOf(float aspect) { return aspect > 1.0 ? vec2(1.0 / aspect, 1.0) : vec2(1.0, aspect); }
vec2 giantAt(vec2 uv, vec2 k, float phase) { return fieldAt((uv * 2.0 - 1.0) / k, ${GIANT_SCALE.toFixed(1)}, phase, vec3(1.0), vec3(1.0), vec2(${PLAIN.range.toFixed(2)}, 0.0)) * k * 0.5; }`;
