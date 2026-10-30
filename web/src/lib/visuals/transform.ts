// Transform (decision 0057 §7b3): light or a picture moved across and up, scaled and turned about the middle. One
// meaning in both: the square space of the points' camera (nodes/points.ts: centred, the screen's short side −1..1),
// Across and Up in the screen's halves (1: the middle to the right edge, the top), Scale about the middle, Turn in turns
// counter-clockwise. On the light it is the points' camera's view (where each point lands, as the camera frames the
// flame); on a picture, one pass reading where each pixel came from. Pure: no GL here.

/** An affine map of the square space: `m` (column-major 2×2, as a GLSL mat2) then `at`. */
export type Affine = { m: [number, number, number, number]; at: [number, number] };

export const IDENTITY: Affine = { m: [1, 0, 0, 1], at: [0, 0] };

/** The square space to the screen's halves at `aspect` (width / height): the long side's −1..1 the screen. */
export const toScreen = (aspect: number): [number, number] => (aspect > 1 ? [1 / aspect, 1] : [1, aspect]);

/** Whether a Transform's knobs move nothing (its picture is its input's). */
export const transformIdle = (k: Record<string, number>) => (k.x ?? 0) === 0 && (k.y ?? 0) === 0 && (k.scale ?? 1) === 1 && (k.rotate ?? 0) === 0;

/** A Transform's map at `aspect`: turned and scaled about the middle, then moved. */
export function transformOf(k: Record<string, number>, aspect: number): Affine {
  const s = k.scale ?? 1;
  const a = 2 * Math.PI * (k.rotate ?? 0);
  const [c, n] = [Math.cos(a) * s, Math.sin(a) * s];
  const [kx, ky] = toScreen(aspect);
  return { m: [c, n, -n, c], at: [(k.x ?? 0) / kx, (k.y ?? 0) / ky] };
}

/** `b` after `a` (not `then`: a module exporting one is a promise to `await import`). */
export function chain(a: Affine, b: Affine): Affine {
  const [a0, a1, a2, a3] = a.m;
  const [b0, b1, b2, b3] = b.m;
  return {
    m: [b0 * a0 + b2 * a1, b1 * a0 + b3 * a1, b0 * a2 + b2 * a3, b1 * a2 + b3 * a3],
    at: [b0 * a.at[0] + b2 * a.at[1] + b.at[0], b1 * a.at[0] + b3 * a.at[1] + b.at[1]],
  };
}

/** The map back (where a point came from); the identity where it squashes everything (Scale is 0.25 at least). */
export function inverse(t: Affine): Affine {
  const [a, b, c, d] = t.m;
  const det = a * d - b * c;
  if (Math.abs(det) < 1e-12) return IDENTITY;
  const m: Affine['m'] = [d / det, -b / det, -c / det, a / det];
  return { m, at: [-(m[0] * t.at[0] + m[2] * t.at[1]), -(m[1] * t.at[0] + m[3] * t.at[1])] };
}

/** A point through `t`. */
export const apply = (t: Affine, p: [number, number]): [number, number] => [t.m[0] * p[0] + t.m[2] * p[1] + t.at[0], t.m[1] * p[0] + t.m[3] * p[1] + t.at[1]];
