// The warps a lens takes (decision 0046's final transforms; 0057 §4f, §4g): flam3's variations that bend the whole
// picture into a whole. One place for them: the flame's `apply` draws its points through them, and shape-set.ts makes
// each one's form (the shape it makes of a disc). Pure.

/** The warps by flam3's numbers (flame.ts VARIATIONS has the same). */
export const WARPS = {
  sinusoidal: 1,
  spherical: 2,
  swirl: 3,
  horseshoe: 4,
  polar: 5,
  handkerchief: 6,
  heart: 7,
  disc: 8,
  julia: 13,
  fisheye: 16,
  eyefish: 27,
  bubble: 28,
} as const;

/** A point (x, y) through warp `v` (its r², r and flam3's θ = atan2(x, y) given); null for a variation that is not one
 *  of these. */
export function warpOf(v: number, x: number, y: number, r2: number, r: number, theta: number, rnd: () => number): [number, number] | null {
  switch (v) {
    case WARPS.sinusoidal: return [Math.sin(x), Math.sin(y)];
    case WARPS.spherical: return [x / r2, y / r2];
    case WARPS.swirl: return [x * Math.sin(r2) - y * Math.cos(r2), x * Math.cos(r2) + y * Math.sin(r2)];
    case WARPS.horseshoe: return [((x - y) * (x + y)) / r, (2 * x * y) / r];
    case WARPS.polar: return [theta / Math.PI, r - 1];
    case WARPS.handkerchief: return [r * Math.sin(theta + r), r * Math.cos(theta - r)];
    case WARPS.heart: return [r * Math.sin(theta * r), -r * Math.cos(theta * r)];
    case WARPS.disc: return [(theta / Math.PI) * Math.sin(Math.PI * r), (theta / Math.PI) * Math.cos(Math.PI * r)];
    case WARPS.julia: {
      const om = rnd() < 0.5 ? 0 : Math.PI;
      const sr = Math.sqrt(r);
      return [sr * Math.cos(theta / 2 + om), sr * Math.sin(theta / 2 + om)];
    }
    case WARPS.fisheye: return [(2 / (r + 1)) * y, (2 / (r + 1)) * x];
    case WARPS.eyefish: return [(2 / (r + 1)) * x, (2 / (r + 1)) * y];
    case WARPS.bubble: return [(4 / (r2 + 4)) * x, (4 / (r2 + 4)) * y];
    default: return null;
  }
}
