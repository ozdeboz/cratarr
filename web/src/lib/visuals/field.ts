// Field (the owner's evolving flow field): a 2D flow that changes smoothly over time, the curl of a stream function made
// of a few travelling waves (each its own direction, a little off the others, its own wavelength about Scale and its own
// pace), so it has no sources or sinks: what it carries neither bunches up nor thins out. The same sum here (for points it
// moves) and in GLSL (for Streamlines), in the frame Grid, Instance and Fold use: centred, y up, the short side −1..1.
// Pure.

/** The waves: direction (radians), wavelength (times Scale), pace (times Speed), phase. */
export const WAVES: readonly (readonly [number, number, number, number])[] = [
  [0.0, 1.0, 1.0, 0.0],
  [2.4, 0.73, -1.3, 1.7],
  [4.8, 1.31, 0.8, 3.1],
  [1.1, 0.57, 1.7, 4.4],
  [3.6, 0.89, -0.6, 5.2],
  [5.7, 1.13, 1.1, 2.3],
];

/** The flow at (x, y) for a field of `scale` (its swirls' size, in the frame's units) at `phase` (its time, run on at
 *  its Speed): the velocity, about 1 at its strongest. */
export function flowAt(x: number, y: number, scale: number, phase: number): [number, number] {
  let u = 0;
  let v = 0;
  for (const [dir, length, pace, offset] of WAVES) {
    const k = (2 * Math.PI) / Math.max(0.01, scale * length);
    const kx = k * Math.cos(dir);
    const ky = k * Math.sin(dir);
    const c = Math.cos(kx * x + ky * y + pace * phase + offset) / k;
    u += c * ky;
    v -= c * kx;
  }
  return [u / WAVES.length * 2, v / WAVES.length * 2];
}

/** The same in GLSL: `vec2 fieldAt(vec2 p, float scale, float phase)`. */
export const FIELD_GLSL = `vec2 fieldAt(vec2 p, float scale, float phase) {
  vec2 f = vec2(0.0);
${WAVES.map(([dir, length, pace, offset]) => {
  const c = Math.cos(dir).toFixed(6);
  const s = Math.sin(dir).toFixed(6);
  return `  { float k = 6.28318530718 / max(0.01, scale * ${length.toFixed(6)}); float c = cos(k * (${c} * p.x + ${s} * p.y) + ${pace.toFixed(6)} * phase + ${offset.toFixed(6)}) / k; f += c * vec2(k * ${s}, -k * ${c}); }`;
}).join('\n')}
  return f / ${WAVES.length.toFixed(1)} * 2.0;
}`;
