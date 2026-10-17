// Fractal flames (decision 0046), written from Draves and Reckase, "The Fractal Flame Algorithm" (2003; revised
// 2008): an iterated function system whose transforms are an affine map followed by a weighted sum of nonlinear
// "variations", drawn by the chaos game with a colour coordinate per point. This file holds a flame's description:
// made from a seed (a track's id gives the same flame every time), checked on the CPU before it is shown (a flame that
// collapses to a point or smears into noise is passed over), framed, evolved by the music, and blended into the
// next. The drawing is lib/visuals/render.ts.
import { BANDS } from './bands';

/** The variations, by the paper's numbers (its appendix). The shader (render.ts) implements the same list. */
export const VARIATIONS = {
  linear: 0,
  sinusoidal: 1,
  spherical: 2,
  swirl: 3,
  horseshoe: 4,
  polar: 5,
  handkerchief: 6,
  heart: 7,
  disc: 8,
  spiral: 9,
  hyperbolic: 10,
  diamond: 11,
  ex: 12,
  julia: 13,
  bent: 14,
  fisheye: 16,
  exponential: 18,
  power: 19,
  cosine: 20,
  eyefish: 27,
  bubble: 28,
  cylinder: 29,
  tangent: 42,
  cross: 48,
} as const;
export type Variation = (typeof VARIATIONS)[keyof typeof VARIATIONS];

/** One transform: x' = a·x + b·y + c, y' = d·x + e·y + f, then its variations summed by weight. */
export type Xform = {
  /** a, b, c, d, e, f. */
  affine: [number, number, number, number, number, number];
  /** Variation number to its weight. */
  variations: Map<Variation, number>;
  /** How often it is chosen, against the others' weights. */
  weight: number;
  /** Its hue, a turn of the colour wheel (0..1), and how far a point's colour moves towards it each time (0..1). A
   *  point's colour is a place in the colour plane (OKLab's a and b), not on a line: one moving between two hues goes
   *  paler on the way, never through a third. */
  color: number;
  colorSpeed: number;
  /** Turned by the animation (a symmetry's rotations are not). */
  turns: boolean;
};

/** The lightness and chroma of a flame's colours (OKLCH); each transform has its own hue. */
export type Colours = { light: number; chroma: number };

export type Flame = {
  xforms: Xform[];
  colours: Colours;
  /** Where its points fall, for the camera: centre and the half-size that holds most of them. */
  center: [number, number];
  size: number;
};

// --- A seeded random source (the same seed, the same flame) ---

/** A string to a 32-bit seed (FNV-1a). */
export function seedOf(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Mulberry32: small, fast, and good enough to choose shapes. */
export function randomFrom(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// --- Making a flame ---

// The variations a random flame picks from, with how often: the gentle curving ones most, the ones that tear the
// picture apart (cross, tangent, exponential) rarely.
const PICKS: [Variation, number][] = [
  [VARIATIONS.linear, 6],
  [VARIATIONS.sinusoidal, 4],
  [VARIATIONS.spherical, 5],
  [VARIATIONS.swirl, 4],
  [VARIATIONS.horseshoe, 3],
  [VARIATIONS.polar, 2],
  [VARIATIONS.handkerchief, 2],
  [VARIATIONS.heart, 2],
  [VARIATIONS.disc, 2],
  [VARIATIONS.spiral, 3],
  [VARIATIONS.hyperbolic, 2],
  [VARIATIONS.diamond, 2],
  [VARIATIONS.ex, 2],
  [VARIATIONS.julia, 5],
  [VARIATIONS.bent, 2],
  [VARIATIONS.fisheye, 2],
  [VARIATIONS.exponential, 1],
  [VARIATIONS.power, 2],
  [VARIATIONS.cosine, 1],
  [VARIATIONS.eyefish, 2],
  [VARIATIONS.bubble, 3],
  [VARIATIONS.cylinder, 2],
  [VARIATIONS.tangent, 1],
  [VARIATIONS.cross, 1],
];
const PICK_TOTAL = PICKS.reduce((n, [, w]) => n + w, 0);

function pickVariation(rnd: () => number): Variation {
  let r = rnd() * PICK_TOTAL;
  for (const [v, w] of PICKS) if ((r -= w) < 0) return v;
  return VARIATIONS.linear;
}

function randomXform(rnd: () => number): Xform {
  // A contraction on the whole, turned and sheared at random, not too far from the middle.
  const angle = rnd() * Math.PI * 2;
  const sx = 0.35 + rnd() * 0.65;
  const sy = 0.35 + rnd() * 0.65;
  const shear = (rnd() - 0.5) * 0.8;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const a = sx * cos;
  const b = sy * (shear * cos - sin);
  const d = sx * sin;
  const e = sy * (shear * sin + cos);
  const variations = new Map<Variation, number>();
  const n = rnd() < 0.55 ? 1 : rnd() < 0.75 ? 2 : 3;
  for (let i = 0; i < n; i++) {
    const v = pickVariation(rnd);
    variations.set(v, (variations.get(v) ?? 0) + 0.3 + rnd() * 0.7);
  }
  // The weights add up to about one, so the transform keeps its size.
  const sum = [...variations.values()].reduce((s, w) => s + w, 0);
  for (const [v, w] of variations) variations.set(v, w / sum);
  return {
    affine: [a, b, (rnd() - 0.5) * 1.6, d, e, (rnd() - 0.5) * 1.6],
    variations,
    weight: 0.25 + rnd(),
    color: rnd(),
    colorSpeed: 0.3 + rnd() * 0.5,
    turns: true,
  };
}

/** Rotational symmetry as flam3 adds it: n−1 pure rotations, which keep a point's colour and do not turn. */
function symmetry(order: number, weight: number): Xform[] {
  const out: Xform[] = [];
  for (let k = 1; k < order; k++) {
    const t = (2 * Math.PI * k) / order;
    out.push({
      affine: [Math.cos(t), -Math.sin(t), 0, Math.sin(t), Math.cos(t), 0],
      variations: new Map([[VARIATIONS.linear, 1]]),
      weight,
      color: 0,
      colorSpeed: 0,
      turns: false,
    });
  }
  return out;
}

/** A flame's colours (OKLCH, the perceptual lightness, chroma and hue): every transform's at one lightness and one
 *  chroma, so they differ in hue alone. */
function randomColours(rnd: () => number): Colours {
  return { light: 0.7 + rnd() * 0.1, chroma: 0.12 + rnd() * 0.05 };
}

/** The flame of a seed: made at random from it until one passes the check, then framed. */
export function flameFrom(seed: number): Flame {
  const rnd = randomFrom(seed);
  let best: { flame: Flame; score: number } | null = null;
  for (let attempt = 0; attempt < 40; attempt++) {
    // Three transforms, one for each of the music's bands, low to high (decision 0046, amended: `evolved`).
    const count = BANDS;
    const xforms = Array.from({ length: count }, () => randomXform(rnd));
    // Each transform its own hue, evenly round the wheel from a random start (a little off even, by the seed), so the
    // parts of the flame, and the bands, are as far apart in colour as they can be.
    const offset = rnd();
    xforms.forEach((x, i) => (x.color = (offset + i / count + (rnd() - 0.5) * 0.04 + 1) % 1));
    const order = rnd() < 0.45 ? 1 : 2 + Math.floor(rnd() * 5);
    const mean = xforms.reduce((s, x) => s + x.weight, 0) / count;
    xforms.push(...symmetry(order, mean));
    const flame: Flame = { xforms, colours: randomColours(rnd), center: [0, 0], size: 1 };
    const look = survey(flame, seed + attempt);
    if (!look) continue;
    flame.center = look.center;
    flame.size = look.size;
    if (look.score > 0.65) return flame;
    if (!best || look.score > best.score) best = { flame, score: look.score };
  }
  return best?.flame ?? fallback();
}

/** A flame that always draws: a swirled, spherical four-fold star. */
function fallback(): Flame {
  return {
    xforms: [
      { affine: [0.6, -0.3, 0.2, 0.3, 0.6, 0.1], variations: new Map([[VARIATIONS.spherical, 0.6], [VARIATIONS.swirl, 0.4]]), weight: 1, color: 0.1, colorSpeed: 0.5, turns: true },
      { affine: [0.5, 0.4, -0.5, -0.4, 0.5, 0.3], variations: new Map([[VARIATIONS.julia, 1]]), weight: 0.7, color: 0.43, colorSpeed: 0.5, turns: true },
      { affine: [0.4, 0, 0.4, 0, 0.4, -0.4], variations: new Map([[VARIATIONS.sinusoidal, 1]]), weight: 0.6, color: 0.77, colorSpeed: 0.5, turns: true },
      ...symmetry(4, 0.77),
    ],
    colours: { light: 0.75, chroma: 0.14 },
    center: [0, 0],
    size: 1.6,
  };
}

// --- The CPU's look at a flame: where its points fall, and whether it is worth showing ---

/** Runs the chaos game on the CPU and measures it: its frame (centre, size) and a score, 0..1 (null: it escapes). */
function survey(flame: Flame, seed: number): { center: [number, number]; size: number; score: number } | null {
  const rnd = randomFrom(seed ^ 0x9e3779b9);
  const pick = picker(flame.xforms);
  const xs: number[] = [];
  const ys: number[] = [];
  let x = rnd() * 2 - 1;
  let y = rnd() * 2 - 1;
  let lost = 0;
  for (let i = 0; i < 12000; i++) {
    const p = apply(flame.xforms[pick(rnd())], x, y, rnd);
    if (!Number.isFinite(p[0]) || !Number.isFinite(p[1]) || Math.abs(p[0]) > 1e4 || Math.abs(p[1]) > 1e4) {
      lost++;
      x = rnd() * 2 - 1;
      y = rnd() * 2 - 1;
      continue;
    }
    [x, y] = p;
    if (i >= 20) {
      xs.push(x);
      ys.push(y);
    }
  }
  if (lost > 600 || xs.length < 5000) return null;
  const sorted = (v: number[]) => [...v].sort((a, b) => a - b);
  const sx = sorted(xs);
  const sy = sorted(ys);
  const q = (s: number[], f: number) => s[Math.floor(f * (s.length - 1))];
  const x0 = q(sx, 0.02);
  const x1 = q(sx, 0.98);
  const y0 = q(sy, 0.02);
  const y1 = q(sy, 0.98);
  const size = Math.max(x1 - x0, y1 - y0) / 2;
  if (!(size > 1e-3)) return null;
  // Coverage of a 40 × 40 grid over the frame: a point or a thin line covers almost nothing, noise covers it all.
  const cells = new Uint32Array(40 * 40);
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  for (let i = 0; i < xs.length; i++) {
    const gx = Math.floor(((xs[i] - cx) / size / 2 + 0.5) * 40);
    const gy = Math.floor(((ys[i] - cy) / size / 2 + 0.5) * 40);
    if (gx >= 0 && gx < 40 && gy >= 0 && gy < 40) cells[gy * 40 + gx]++;
  }
  let hit = 0;
  for (const c of cells) if (c > 0) hit++;
  const coverage = hit / cells.length;
  // Structure: how unevenly the points spread over the cells they reach (a flat fill has none).
  const mean = xs.length / Math.max(1, hit);
  let spread = 0;
  for (const c of cells) if (c > 0) spread += Math.abs(c - mean);
  const unevenness = spread / xs.length;
  // Haze: the share of the cells reached that hold only a trickle of points, an even scatter over the background
  // that shows as grain however long it is drawn.
  let thin = 0;
  for (const c of cells) if (c > 0 && c < mean * 0.2) thin++;
  const haze = thin / Math.max(1, hit);
  // Electric Sheep's flames leave dark around them: a flame filling more than 60 % of its frame scores less.
  const fits = coverage < 0.06 ? coverage / 0.06 : coverage > 0.6 ? Math.max(0, (0.9 - coverage) / 0.3) : 1;
  const structured = Math.min(1, unevenness / 0.6);
  const clear = haze > 0.35 ? Math.max(0, (0.6 - haze) / 0.25) : 1;
  return { center: [cx, cy], size: size * 1.1, score: fits * structured * clear };
}

/** Where a flame's points fall now (as it is drawn: turned, evolved), on the CPU: the middle and half-size of the
 *  frame that holds 96 % of them, and how much of that frame's 32 x 32 cells they reach. The visuals measure their flame
 *  so twice a second and follow it slowly with their framing and exposure: a flame whose transforms turn passes through
 *  shapes three times as large as its own and back (measured: 0.63 to 3.1, 2026-10-18), which showed as flares the
 *  music had no part in. */
export function measure(flame: Flame, steps = 6000): { center: [number, number]; size: number; coverage: number } | null {
  const rnd = randomFrom(0x51ced);
  const pick = picker(flame.xforms);
  const xs: number[] = [];
  const ys: number[] = [];
  let x = rnd() * 2 - 1;
  let y = rnd() * 2 - 1;
  for (let i = 0; i < steps; i++) {
    const p = apply(flame.xforms[pick(rnd())], x, y, rnd);
    if (!Number.isFinite(p[0]) || !Number.isFinite(p[1]) || Math.abs(p[0]) > 1e4 || Math.abs(p[1]) > 1e4) {
      x = rnd() * 2 - 1;
      y = rnd() * 2 - 1;
      continue;
    }
    [x, y] = p;
    if (i >= 20) {
      xs.push(x);
      ys.push(y);
    }
  }
  if (xs.length < steps / 4) return null;
  const sx = [...xs].sort((a, b) => a - b);
  const sy = [...ys].sort((a, b) => a - b);
  const q = (s: number[], f: number) => s[Math.floor(f * (s.length - 1))];
  const center: [number, number] = [(q(sx, 0.02) + q(sx, 0.98)) / 2, (q(sy, 0.02) + q(sy, 0.98)) / 2];
  const size = (Math.max(q(sx, 0.98) - q(sx, 0.02), q(sy, 0.98) - q(sy, 0.02)) / 2) * 1.1;
  if (!(size > 1e-4)) return null;
  const cells = new Uint8Array(32 * 32);
  for (let i = 0; i < xs.length; i++) {
    const gx = Math.floor(((xs[i] - center[0]) / size / 2 + 0.5) * 32);
    const gy = Math.floor(((ys[i] - center[1]) / size / 2 + 0.5) * 32);
    if (gx >= 0 && gx < 32 && gy >= 0 && gy < 32) cells[gy * 32 + gx] = 1;
  }
  return { center, size, coverage: cells.reduce((s, c) => s + c, 0) / cells.length };
}

/** Chooses a transform by weight, given a number in 0..1. */
export function picker(xforms: Xform[]): (r: number) => number {
  const total = xforms.reduce((s, x) => s + x.weight, 0);
  const sums: number[] = [];
  let run = 0;
  for (const x of xforms) sums.push((run += x.weight / total));
  return (r) => {
    for (let i = 0; i < sums.length; i++) if (r < sums[i]) return i;
    return sums.length - 1;
  };
}

/** One transform on one point, on the CPU (the shader in render.ts does the same). */
export function apply(t: Xform, px: number, py: number, rnd: () => number): [number, number] {
  const [a, b, c, d, e, f] = t.affine;
  const x = a * px + b * py + c;
  const y = d * px + e * py + f;
  let ox = 0;
  let oy = 0;
  const r2 = x * x + y * y + 1e-12;
  const r = Math.sqrt(r2);
  const theta = Math.atan2(x, y);
  for (const [v, w] of t.variations) {
    let vx = x;
    let vy = y;
    switch (v) {
      case VARIATIONS.sinusoidal: vx = Math.sin(x); vy = Math.sin(y); break;
      case VARIATIONS.spherical: vx = x / r2; vy = y / r2; break;
      case VARIATIONS.swirl: vx = x * Math.sin(r2) - y * Math.cos(r2); vy = x * Math.cos(r2) + y * Math.sin(r2); break;
      case VARIATIONS.horseshoe: vx = ((x - y) * (x + y)) / r; vy = (2 * x * y) / r; break;
      case VARIATIONS.polar: vx = theta / Math.PI; vy = r - 1; break;
      case VARIATIONS.handkerchief: vx = r * Math.sin(theta + r); vy = r * Math.cos(theta - r); break;
      case VARIATIONS.heart: vx = r * Math.sin(theta * r); vy = -r * Math.cos(theta * r); break;
      case VARIATIONS.disc: vx = (theta / Math.PI) * Math.sin(Math.PI * r); vy = (theta / Math.PI) * Math.cos(Math.PI * r); break;
      case VARIATIONS.spiral: vx = (Math.cos(theta) + Math.sin(r)) / r; vy = (Math.sin(theta) - Math.cos(r)) / r; break;
      case VARIATIONS.hyperbolic: vx = Math.sin(theta) / r; vy = r * Math.cos(theta); break;
      case VARIATIONS.diamond: vx = Math.sin(theta) * Math.cos(r); vy = Math.cos(theta) * Math.sin(r); break;
      case VARIATIONS.ex: {
        const p0 = Math.sin(theta + r) ** 3;
        const p1 = Math.cos(theta - r) ** 3;
        vx = r * (p0 + p1);
        vy = r * (p0 - p1);
        break;
      }
      case VARIATIONS.julia: {
        const om = rnd() < 0.5 ? 0 : Math.PI;
        const sr = Math.sqrt(r);
        vx = sr * Math.cos(theta / 2 + om);
        vy = sr * Math.sin(theta / 2 + om);
        break;
      }
      case VARIATIONS.bent: vx = x < 0 ? 2 * x : x; vy = y < 0 ? y / 2 : y; break;
      case VARIATIONS.fisheye: vx = (2 / (r + 1)) * y; vy = (2 / (r + 1)) * x; break;
      case VARIATIONS.exponential: { const k = Math.exp(x - 1); vx = k * Math.cos(Math.PI * y); vy = k * Math.sin(Math.PI * y); break; }
      case VARIATIONS.power: { const k = r ** Math.sin(theta); vx = k * Math.cos(theta); vy = k * Math.sin(theta); break; }
      case VARIATIONS.cosine: vx = Math.cos(Math.PI * x) * Math.cosh(y); vy = -Math.sin(Math.PI * x) * Math.sinh(y); break;
      case VARIATIONS.eyefish: vx = (2 / (r + 1)) * x; vy = (2 / (r + 1)) * y; break;
      case VARIATIONS.bubble: vx = (4 / (r2 + 4)) * x; vy = (4 / (r2 + 4)) * y; break;
      case VARIATIONS.cylinder: vx = Math.sin(x); vy = y; break;
      case VARIATIONS.tangent: vx = Math.sin(x) / Math.cos(y); vy = Math.tan(y); break;
      case VARIATIONS.cross: { const k = 1 / Math.abs(x * x - y * y + 1e-12); vx = k * x; vy = k * y; break; }
    }
    ox += w * vx;
    oy += w * vy;
  }
  return [ox, oy];
}

// --- Animation and blending ---

/** The variation a band's loudness (decision 0046, amended: low, middle and high; a flame's first three transforms
 *  are theirs) brings into its transform: round and heavy for the low band (bubble), a twist for
 *  the middle (swirl), a fine ripple for the high (sinusoidal). */
const BAND_VARIATIONS: Variation[] = [VARIATIONS.bubble, VARIATIONS.swirl, VARIATIONS.sinusoidal];

/** How a band's transform has evolved: turned by `turn` (radians, as Electric Sheep's loops turn the linear part), that
 *  part scaled by `size`, `morph` (0..1) of its variations given over to its band's own, and its hue moved by `hue` (a
 *  turn of the wheel). */
export type Part = { turn: number; size: number; morph: number; hue: number };

/** The most a transform's linear part may stretch anything (its larger singular value): under 1, every transform pulls
 *  points together, so turning and growing never carry the flame through a shape that flies apart over the screen. */
const STRETCH = 0.95;

/** The flame with each band's transform evolved by its part (the symmetry's rotations stay as they are). `purity` (0..1):
 *  how far each band's transform takes a point's colour all the way to its own (1: every point its last transform's
 *  colour, so each band's part is its colour alone). */
export function evolved(flame: Flame, parts: Part[], purity = 0): Flame {
  return {
    ...flame,
    xforms: flame.xforms.map((t, i) => {
      const p = parts[i];
      if (!t.turns || !p) return t;
      const cos = Math.cos(p.turn) * p.size;
      const sin = Math.sin(p.turn) * p.size;
      const [a, b, c, d, e, f] = t.affine;
      let variations = t.variations;
      if (p.morph > 0) {
        variations = new Map([...t.variations].map(([v, w]) => [v, w * (1 - p.morph)]));
        const own = BAND_VARIATIONS[i];
        variations.set(own, (variations.get(own) ?? 0) + p.morph);
      }
      const linear = contracted(cos * a - sin * d, cos * b - sin * e, sin * a + cos * d, sin * b + cos * e);
      return {
        ...t,
        affine: [linear[0], linear[1], c, linear[2], linear[3], f],
        variations,
        color: (((t.color + p.hue) % 1) + 1) % 1,
        colorSpeed: t.colorSpeed + purity * (1 - t.colorSpeed),
      };
    }),
  };
}

/** A flame between two, `k` of the way from `from` to `to` (0..1). The transforms are paired in order (a missing one
 *  weighs nothing); each affine column is blended by angle and by the log of its length, which keeps a transform from
 *  folding flat on the way (flam3's "log" interpolation does this). */
export function between(from: Flame, to: Flame, k: number): Flame {
  const n = Math.max(from.xforms.length, to.xforms.length);
  const none = (t: Xform): Xform => ({ ...t, weight: 0 });
  const xforms: Xform[] = [];
  for (let i = 0; i < n; i++) {
    const p = from.xforms[i] ?? none(to.xforms[i]);
    const q = to.xforms[i] ?? none(from.xforms[i]);
    const variations = new Map<Variation, number>();
    for (const v of new Set([...p.variations.keys(), ...q.variations.keys()])) {
      variations.set(v, mix(p.variations.get(v) ?? 0, q.variations.get(v) ?? 0, k));
    }
    xforms.push({
      affine: blendAffine(p.affine, q.affine, k),
      variations,
      weight: mix(p.weight, q.weight, k),
      color: mixTurn(p.color, q.color, k),
      colorSpeed: mix(p.colorSpeed, q.colorSpeed, k),
      turns: k < 0.5 ? p.turns : q.turns,
    });
  }
  return {
    xforms,
    colours: { light: mix(from.colours.light, to.colours.light, k), chroma: mix(from.colours.chroma, to.colours.chroma, k) },
    center: [mix(from.center[0], to.center[0], k), mix(from.center[1], to.center[1], k)],
    size: Math.exp(mix(Math.log(from.size), Math.log(to.size), k)),
  };
}

const mix = (a: number, b: number, k: number) => a + (b - a) * k;
/** Between two turns of the wheel, the short way round. */
const mixTurn = (a: number, b: number, k: number) => {
  const d = b - a - Math.round(b - a);
  return (((a + d * k) % 1) + 1) % 1;
};

/** A 2 x 2 linear part (a, b; d, e), scaled down where it stretches anything more than `STRETCH`. */
function contracted(a: number, b: number, d: number, e: number): [number, number, number, number] {
  // The larger singular value, from the sums and differences of the matrix's entries.
  const s = Math.hypot(a + e, d - b) / 2 + Math.hypot(a - e, d + b) / 2;
  const k = s > STRETCH ? STRETCH / s : 1;
  return [a * k, b * k, d * k, e * k];
}

function blendAffine(p: Xform['affine'], q: Xform['affine'], k: number): Xform['affine'] {
  // Columns (a, d) and (b, e) in polar form; the offset (c, f) straight.
  const col = (x0: number, y0: number, x1: number, y1: number): [number, number] => {
    const a0 = Math.atan2(y0, x0);
    let a1 = Math.atan2(y1, x1);
    if (a1 - a0 > Math.PI) a1 -= 2 * Math.PI;
    if (a0 - a1 > Math.PI) a1 += 2 * Math.PI;
    const l = Math.exp(mix(Math.log(Math.hypot(x0, y0) + 1e-9), Math.log(Math.hypot(x1, y1) + 1e-9), k));
    const a = mix(a0, a1, k);
    return [l * Math.cos(a), l * Math.sin(a)];
  };
  const [a, d] = col(p[0], p[3], q[0], q[3]);
  const [b, e] = col(p[1], p[4], q[1], q[4]);
  return [a, b, mix(p[2], q[2], k), d, e, mix(p[5], q[5], k)];
}

/** A colour (sRGB, 0..1 each) from OKLCH: its lightness, chroma and hue (a turn of the wheel). The shader in render.ts
 *  does the same. */
export function colourOf(light: number, chroma: number, hue: number): [number, number, number] {
  const a = chroma * Math.cos(2 * Math.PI * hue);
  const b = chroma * Math.sin(2 * Math.PI * hue);
  const l = (light + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (light - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (light - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const linear = [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
  return linear.map((v) => Math.min(1, Math.max(0, v)) ** (1 / 2.2)) as [number, number, number];
}
