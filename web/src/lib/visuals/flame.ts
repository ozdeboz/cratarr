// Fractal flames (decision 0046), written from Draves and Reckase, "The Fractal Flame Algorithm" (2003; revised
// 2008): an iterated function system whose transforms are an affine map followed by a weighted sum of nonlinear
// "variations", drawn by the chaos game with a colour coordinate per point. This file holds a flame's description:
// made from a preset of the look's settings alone (a track's id gives the same preset every time, checked on the CPU
// before it is shown: one whose flame collapses to a point or smears into noise is passed over), framed, evolved by the
// music, and blended into the next. The drawing is lib/visuals/render.ts.
import { DRAWN_ROOM } from './budget';
import { ENGINE_EFFECTS, ENGINES, LOOP_EFFECTS, settingsReport } from './effects';
import { LENS_SHAPES, lensInto, markOf } from './shape-set';
import { warpOf } from './warps';

export { MARK, markOf } from './shape-set';


/** A flame's parts: its first transforms, one for each of the music's six bands, low to high (decision 0046, amended). */
export const PARTS = 6;

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
  waves: 15,
  fisheye: 16,
  popcorn: 17,
  exponential: 18,
  power: 19,
  cosine: 20,
  rings: 21,
  fan: 22,
  eyefish: 27,
  bubble: 28,
  cylinder: 29,
  noise: 31,
  blur: 34,
  arch: 41,
  tangent: 42,
  square: 43,
  rays: 44,
  blade: 45,
  secant2: 46,
  twintrian: 47,
  cross: 48,
  // Cratarr's own (not flam3's): its mark (the Cratarr lens, `MARK`).
  cratarr: 99,
} as const;
/** The lens's shapes of the one set (0057 §4g; shape-set.ts `LENS_SHAPES`, in its order): a point sent into the shape.
 *  Numbers of their own, past flam3's and Cratarr's (flam3 has a Cross variation of its own). */
export const LENS_SHAPE_VARIATIONS = { polygon: 100, star: 101, ring: 102, cross: 103, arrow: 104, crescent: 105, drop: 106, heart: 107 } as const;
const LENS_SHAPE_FIRST = LENS_SHAPE_VARIATIONS.polygon;
export type Variation = (typeof VARIATIONS)[keyof typeof VARIATIONS] | (typeof LENS_SHAPE_VARIATIONS)[keyof typeof LENS_SHAPE_VARIATIONS];

export type Affine = [number, number, number, number, number, number];

/** One transform: x' = a·x + b·y + c, y' = d·x + e·y + f, then its variations summed by weight, then (flam3's "post")
 *  another affine map, where it has one. */
export type Xform = {
  /** a, b, c, d, e, f. */
  affine: Affine;
  post?: Affine;
  /** Variation number to its weight. */
  variations: Map<Variation, number>;
  /** How often it is chosen, against the others' weights. */
  weight: number;
  /** Its hue, a turn of the colour wheel (0..1), and how far a point's colour moves towards it each time (0..1). A
   *  point's colour is a place in the colour plane (OKLab's a and b), not on a line: one moving between two hues goes
   *  paler on the way, never through a third. */
  color: number;
  colorSpeed: number;
  /** How far out its colour is (0..1 of the flame's chroma): two parts of one hue told apart by it (a palette family's
   *  pairs). */
  colorReach?: number;
  /** Turned by the animation (a symmetry's rotations and mirrors are not). */
  turns: boolean;
  /** A lens's shape's sides and depth (its Polygon and Star: the Lens's Sides and Depth settings). */
  shape?: { sides: number; depth: number };
};

/** The lightness and chroma of a flame's colours (OKLCH); each transform has its own hue. `lights`: each part's own
 *  lightness, where they differ (a cover's colours, the fire's), in place of `light`. */
export type Colours = { light: number; chroma: number; lights?: number[] };

export type Flame = {
  xforms: Xform[];
  /** flam3's final transform: applied to every point as it is drawn, not as it goes on (a warp of the whole picture). */
  final?: Xform;
  /** Its symmetry, as made (the visuals' Symmetry and Mirror settings may ask another): rotations of this order, and
   *  mirrors across as many lines. */
  symmetry: { order: number; mirror: boolean };
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

// The variations a random flame picks from, with how often: nearly evenly (three times as often as the rest only for the
// plainest, and a third for the ones that tear the picture apart or scatter it into dust).
const PICKS: [Variation, number][] = [
  [VARIATIONS.linear, 4],
  [VARIATIONS.sinusoidal, 3],
  [VARIATIONS.spherical, 3],
  [VARIATIONS.swirl, 3],
  [VARIATIONS.horseshoe, 3],
  [VARIATIONS.polar, 3],
  [VARIATIONS.handkerchief, 3],
  [VARIATIONS.heart, 3],
  [VARIATIONS.disc, 3],
  [VARIATIONS.spiral, 3],
  [VARIATIONS.hyperbolic, 3],
  [VARIATIONS.diamond, 3],
  [VARIATIONS.ex, 3],
  [VARIATIONS.julia, 3],
  [VARIATIONS.bent, 3],
  [VARIATIONS.waves, 3],
  [VARIATIONS.fisheye, 3],
  [VARIATIONS.popcorn, 2],
  [VARIATIONS.exponential, 2],
  [VARIATIONS.power, 3],
  [VARIATIONS.cosine, 2],
  [VARIATIONS.rings, 3],
  [VARIATIONS.fan, 3],
  [VARIATIONS.eyefish, 3],
  [VARIATIONS.bubble, 3],
  [VARIATIONS.cylinder, 3],
  [VARIATIONS.noise, 1],
  [VARIATIONS.blur, 1],
  [VARIATIONS.arch, 2],
  [VARIATIONS.tangent, 1],
  [VARIATIONS.square, 1],
  [VARIATIONS.rays, 1],
  [VARIATIONS.blade, 2],
  [VARIATIONS.secant2, 1],
  [VARIATIONS.twintrian, 2],
  [VARIATIONS.cross, 1],
];
const PICK_TOTAL = PICKS.reduce((n, [, w]) => n + w, 0);

/** A variation by `u` (0..1) through the picks' weights. */
function pickVariation(u: number): Variation {
  let r = u * PICK_TOTAL;
  for (const [v, w] of PICKS) if ((r -= w) < 0) return v;
  return VARIATIONS.linear;
}

/** An affine map turned by `angle`, scaled by `sx` and `sy`, sheared, and moved by (c, f). */
function affineOf(angle: number, sx: number, sy: number, shear: number, c: number, f: number): Affine {
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  return [sx * cos, sy * (shear * cos - sin), c, sx * sin, sy * (shear * sin + cos), f];
}

/** How many numbers each part draws from its form: always as many, so a slider moved changes no other part's draws. */
const DRAWS = 22;

/** A band's part: its form's draws (`r`, 0..1 each) about the preset's Detail, Twist and Warp. */
function partOf(r: number[], p: Preset): Xform {
  // Detail: how far it contracts (0.3, tight, to 1, open, as the random ones ranged), each part a quarter either side.
  const scale = 0.3 + 0.7 * p.detail;
  const sx = Math.min(1, Math.max(0.15, scale + (r[1] - 0.5) * 0.5));
  const sy = Math.min(1, Math.max(0.15, scale + (r[2] - 0.5) * 0.5));
  // Twist: how far it turns, each part its own way (at the most any way round, as the random ones did); sheared and
  // moved off the middle by its form.
  const affine = affineOf((r[0] * 2 - 1) * Math.PI * p.twist, sx, sy, (r[3] - 0.5) * 0.8, (r[4] - 0.5) * 1.6, (r[5] - 0.5) * 1.6);
  // Its variations: one to three, by its form, their weights adding up to Warp's share (so the part keeps its size), the
  // rest plain.
  const kinds = new Map<Variation, number>();
  const n = r[6] < 0.4 ? 1 : r[7] < 0.6 ? 2 : 3;
  for (let i = 0; i < n; i++) {
    const v = pickVariation(r[8 + i]);
    kinds.set(v, (kinds.get(v) ?? 0) + 0.3 + r[11 + i] * 0.7);
  }
  const sum = [...kinds.values()].reduce((s, w) => s + w, 0);
  const variations = new Map<Variation, number>();
  for (const [v, w] of kinds) variations.set(v, (w / sum) * p.warp);
  if (p.warp < 1) variations.set(VARIATIONS.linear, (variations.get(VARIATIONS.linear) ?? 0) + 1 - p.warp);
  // A third of them a post map: a turn (by Twist) and a little scaling after the variations, which bends their shapes
  // round.
  const post = r[14] < 0.35 ? affineOf((r[15] * 2 - 1) * Math.PI * p.twist, 0.8 + r[16] * 0.4, 0.8 + r[17] * 0.4, 0, (r[18] - 0.5) * 0.4, (r[19] - 0.5) * 0.4) : undefined;
  return { affine, post, variations, weight: 0.25 + r[20], color: 0, colorSpeed: 0.3 + r[21] * 0.5, turns: true };
}

// The variations a final transform takes: the ones that warp the whole picture into a whole (a sphere, a swirl, a lens),
// not the ones that scatter it.
const FINALS: Variation[] = [
  VARIATIONS.spherical,
  VARIATIONS.swirl,
  VARIATIONS.julia,
  VARIATIONS.fisheye,
  VARIATIONS.eyefish,
  VARIATIONS.bubble,
  VARIATIONS.polar,
  VARIATIONS.disc,
  VARIATIONS.sinusoidal,
  VARIATIONS.horseshoe,
  VARIATIONS.handkerchief,
  // The real heart, at flam3's heart's place (the owner, 2026-10-20: "real heart only"; the parts keep flam3's).
  LENS_SHAPE_VARIATIONS.heart,
  VARIATIONS.cratarr,
  // The rest of the one set of shapes (0057 §4g), after Cratarr's, so every other track's look stays as it was drawn.
  LENS_SHAPE_VARIATIONS.polygon,
  LENS_SHAPE_VARIATIONS.star,
  LENS_SHAPE_VARIATIONS.ring,
  LENS_SHAPE_VARIATIONS.cross,
  LENS_SHAPE_VARIATIONS.arrow,
  LENS_SHAPE_VARIATIONS.crescent,
  LENS_SHAPE_VARIATIONS.drop,
];
/** The finals a preset draws among: the ones before Cratarr's, so every track's own look stays as it was drawn (the
 *  Cratarr lens is one to pick). */
/** The lenses a track's look draws (the shapes' after them are chosen by hand). */
export const DRAWN_FINALS = FINALS.indexOf(VARIATIONS.cratarr);
/** The Lens module's lens (its Kind, 1 the first) as the variation it is (0.10: a picture through it). */
export const lensWarpOf = (kind: number): Variation => FINALS[(((Math.round(kind) - 1) % FINALS.length) + FINALS.length) % FINALS.length];

/** The finals' names, for the Lens setting (1 is the first). */
export const LENSES = FINALS.map((v) => {
  const name = Object.entries({ ...VARIATIONS, ...LENS_SHAPE_VARIATIONS }).find(([, n]) => n === v)![0];
  return name[0].toUpperCase() + name.slice(1);
});

/** A final transform (the Lens setting's `lens`, one of `FINALS`), by its form's draws (`f`): near the identity, then
 *  the warp, up to half of it the plain picture. A shape's (Cratarr's and the rest of the set's) all of it: any plain
 *  picture added would fill around it; Polygon's and Star's by the Lens's sides and depth. */
function finalOf(f: number[], lens: number, shape: { sides: number; depth: number }): Xform {
  const warp = FINALS[(lens - 1) % FINALS.length];
  const whole = warp >= VARIATIONS.cratarr;
  return {
    affine: affineOf(f[0] * Math.PI * 2, 0.8 + f[1] * 0.5, 0.8 + f[2] * 0.5, (f[3] - 0.5) * 0.3, (f[4] - 0.5) * 0.3, (f[5] - 0.5) * 0.3),
    ...(warp >= LENS_SHAPE_FIRST ? { shape } : {}),
    variations:
      whole
        ? new Map([[warp, 1]])
        : new Map([
            [warp, 0.5 + f[6] * 0.5],
            [VARIATIONS.linear, f[7] * 0.5],
          ]),
    weight: 0,
    color: 0,
    colorSpeed: 0,
    turns: false,
  };
}

/** Symmetry as flam3 adds it: order − 1 pure rotations, and with `mirror` (dihedral symmetry) as many reflections, across
 *  lines `order` apart; they keep a point's colour and do not turn. */
function symmetry(order: number, mirror: boolean, weight: number): Xform[] {
  const out: Xform[] = [];
  const plain = (affine: Affine): Xform => ({ affine, variations: new Map([[VARIATIONS.linear, 1]]), weight, color: 0, colorSpeed: 0, turns: false });
  for (let k = 1; k < order; k++) {
    const t = (2 * Math.PI * k) / order;
    out.push(plain([Math.cos(t), -Math.sin(t), 0, Math.sin(t), Math.cos(t), 0]));
  }
  if (mirror)
    for (let k = 0; k < order; k++) {
      const t = (2 * Math.PI * k) / order;
      out.push(plain([Math.cos(t), Math.sin(t), 0, Math.sin(t), -Math.cos(t), 0]));
    }
  return out;
}

/** The flame with another symmetry: its parts as they are, the rotations and mirrors made anew (the visuals' Symmetry and
 *  Mirror settings; `order` 1 and no mirror: none). */
export function withSymmetry(flame: Flame, order: number, mirror: boolean): Flame {
  const parts = flame.xforms.filter((x) => x.turns);
  const mean = parts.reduce((s, x) => s + x.weight, 0) / Math.max(1, parts.length);
  return { ...flame, xforms: [...parts, ...symmetry(order, mirror, mean)], symmetry: { order, mirror } };
}

/** The parts' hues and reaches (OKLCH) by the Family setting, every family keeping the bands told apart: the spectrum
 *  (six hues evenly round the wheel), an arc (the six spread over a third to over half of it, a warm or a cool sweep),
 *  two tones (two hues across the wheel, the bands taking them in turn, near and far), three (each for two neighbouring
 *  bands, near and far). From `hue` (the Hue setting); the arc's span, the tones' distance and each part's own hair of
 *  difference from the form's draws (`f`). */
function huesOf(family: number, hue: number, f: number[]): { color: number; reach: number }[] {
  return Array.from({ length: PARTS }, (_, i) => {
    let h: number;
    let reach = 1;
    if (family === 0) h = hue + i / PARTS;
    else if (family === 1) h = hue + (i / (PARTS - 1)) * (0.33 + f[0] * 0.22);
    else if (family === 2) {
      h = hue + (i % 2) * (0.42 + f[1] * 0.08);
      reach = i % 4 < 2 ? 1 : 0.55;
    } else {
      h = hue + Math.floor(i / 2) / 3;
      reach = i % 2 ? 0.55 : 1;
    }
    return { color: (((h + (f[2 + i] - 0.5) * 0.03) % 1) + 1) % 1, reach };
  });
}

/** A flame's look, all of it as the visuals' settings show it (decision 0046, amended): a track's key or the dice give
 *  one (`presetOf`) and the flame is made from it alone (`flameOf`), so every slider moves what is shown and nothing about
 *  the look is hidden in a seed.
 *  - form: which variations each part is made of, and each part's own difference from the sliders' values (a choice,
 *    not a dial: neighbouring numbers are unrelated forms, 0 to `FORMS` - 1);
 *  - detail, twist, warp (0..1): how far the parts contract (tight, dense cores to large, wispy forms), how far they
 *    turn (straight arms to spirals), how much of each is its variations rather than plain (gentle to wild);
 *  - symmetry (1 to 8 arms), mirror (0 or 1), final (the Lens: 0 none, else one of `LENSES`, 1 the first);
 *  - family (0 to 3: spectrum, arc, two tones, three; 4 and 5, the Colours' Cover and Fire, chosen only by hand), hue (where the colours start, a turn of the wheel), lightness and
 *    vividness (0..1: OKLCH lightness 0.55 to 0.85, chroma 0.06 to 0.26);
 *  - spread and purity, as `evolved` takes them. */
export type Preset = {
  form: number;
  detail: number;
  twist: number;
  warp: number;
  symmetry: number;
  mirror: number;
  final: number;
  family: number;
  hue: number;
  lightness: number;
  vividness: number;
  spread: number;
  purity: number;
  /** The effects (65, effects.ts): the picture's three engines (Kaleidoscope, EYEDO, Hydra) and the loop, each its
   *  sub-effect (its place in the engine's list, 0: none) and its amount (0..1). */
  fx1: number;
  fx1Amount: number;
  fx2: number;
  fx2Amount: number;
  fx3: number;
  fx3Amount: number;
  fxLoop: number;
  fxLoopAmount: number;
  /** Each engine's (and the loop's) sub-effect's two knobs (0..1, the half way a middle setting). */
  fx1K1: number;
  fx1K2: number;
  fx2K1: number;
  fx2K2: number;
  fx3K1: number;
  fx3K2: number;
  fxLoopK1: number;
  fxLoopK2: number;
  /** Facets (65): how much each cell of a lattice takes on one light where the light flows (0..1: none to hard tiles),
   *  and the lattice (0 square, 1 hex). */
  facets: number;
  lattice: number;
  /** The Lens's Polygon and Star (0057 §4g): their sides (3–48, through the fractions) and the star's depth (0..1: 0 the
   *  polygon). Only those two lenses read them; a draw leaves them as they are. */
  lensSides: number;
  lensDepth: number;
};
/** The Lens's sides and depth as a draw leaves them (a pentagon; a star half deep). */
export const LENS_SIDES = 5;
export const LENS_DEPTH = 0.5;
export const PRESET_KEYS = [
  'form',
  'detail',
  'twist',
  'warp',
  'symmetry',
  'mirror',
  'final',
  'family',
  'hue',
  'lightness',
  'vividness',
  'spread',
  'purity',
  'fx1',
  'fx1Amount',
  'fx2',
  'fx2Amount',
  'fx3',
  'fx3Amount',
  'fxLoop',
  'fxLoopAmount',
  'fx1K1',
  'fx1K2',
  'fx2K1',
  'fx2K2',
  'fx3K1',
  'fx3K2',
  'fxLoopK1',
  'fxLoopK2',
  'facets',
  'lattice',
  'lensSides',
  'lensDepth',
] as const;
/** No effects. */
const NO_EFFECTS = {
  fx1: 0,
  fx1Amount: 0,
  fx2: 0,
  fx2Amount: 0,
  fx3: 0,
  fx3Amount: 0,
  fxLoop: 0,
  fxLoopAmount: 0,
  fx1K1: 0.5,
  fx1K2: 0.5,
  fx2K1: 0.5,
  fx2K2: 0.5,
  fx3K1: 0.5,
  fx3K2: 0.5,
  fxLoopK1: 0.5,
  fxLoopK2: 0.5,
};
export const FORMS = 1000;

/** A preset that always draws something, for when no draw from a seed passes the check. */
const STANDBY: Preset = { form: 0, detail: 0.5, twist: 0.7, warp: 0.8, symmetry: 4, mirror: 0, final: 0, family: 0, hue: 0, lightness: 0.5, vividness: 0.5, spread: 1, purity: 0.75, ...NO_EFFECTS, facets: 0, lattice: 0, lensSides: LENS_SIDES, lensDepth: LENS_DEPTH };

/** Drawn effects held within the room the flame leaves under each limit (budget.ts `DRAWN_ROOM`), so a track's own
 *  look never trips the sidechaining, quiet or loud: every amount (the picture's and the loop's) by one factor, the least
 *  any meter needs (made again until it holds, for the reports not quite in step with their amounts: Echo's zoom,
 *  Magnify's power). The owner's own effects are not held to it. */
export function withinRoom<T extends Pick<Preset, keyof typeof NO_EFFECTS>>(fx: T): T {
  const scaled = (f: number): T => ({ ...fx, fx1Amount: fx.fx1Amount * f, fx2Amount: fx.fx2Amount * f, fx3Amount: fx.fx3Amount * f, fxLoopAmount: fx.fxLoopAmount * f });
  let f = 1;
  for (let i = 0; i < 6; i++) {
    const r = settingsReport(scaled(f), 1);
    const g = Math.min(1, ...(Object.keys(DRAWN_ROOM) as (keyof typeof DRAWN_ROOM)[]).map((m) => (r[m]! > 0 ? DRAWN_ROOM[m] / r[m]! : 1)));
    if (g > 1 - 1e-9) break;
    f *= g;
  }
  return scaled(f);
}

/** A look's effects (effects.ts), drawn from a stream of their own, so a seed's other draws (its form, its colours …)
 *  stay as they were: one look in three one engine or two on the picture, one in five an effect on the loop, at no more
 *  than four fifths of their amount, then held within the room the flame leaves (`withinRoom`). */
export function effectsDrawn(seed: number): Pick<Preset, keyof typeof NO_EFFECTS> {
  const rnd = randomFrom((seed ^ 0x5eedf00d) >>> 0);
  // One engine or two (never the same twice), each one of its sub-effects.
  const place = (n: number) => 1 + Math.floor(rnd() * n);
  const n = rnd() < 0.67 ? 0 : rnd() < 0.6 ? 1 : 2;
  const out = { ...NO_EFFECTS };
  const first = Math.floor(rnd() * ENGINES.length);
  const engines = n === 0 ? [] : n === 1 ? [first] : [first, (first + 1 + Math.floor(rnd() * (ENGINES.length - 1))) % ENGINES.length];
  for (const k of engines) {
    const key = (['fx1', 'fx2', 'fx3'] as const)[k];
    out[key] = place(ENGINE_EFFECTS[ENGINES[k].key].length);
    out[`${key}Amount` as 'fx1Amount'] = 0.3 + rnd() * 0.5;
  }
  if (rnd() < 0.2) [out.fxLoop, out.fxLoopAmount] = [place(LOOP_EFFECTS.length), 0.2 + rnd() * 0.4];
  // Their knobs, drawn last (so the draws above stay as they were): somewhere in their middle half.
  for (const k of ['fx1K1', 'fx1K2', 'fx2K1', 'fx2K2', 'fx3K1', 'fx3K2', 'fxLoopK1', 'fxLoopK2'] as const) out[k] = 0.25 + rnd() * 0.5;
  return withinRoom(out);
}

/** A look's Facets, from a stream of their own too: one look in five, at a third to nine tenths, square or hex alike. */
function facetsDrawn(seed: number): Pick<Preset, 'facets' | 'lattice'> {
  const rnd = randomFrom((seed ^ 0x0fac37ed) >>> 0);
  return rnd() < 0.2 ? { facets: 0.3 + rnd() * 0.6, lattice: rnd() < 0.5 ? 1 : 0 } : { facets: 0, lattice: 0 };
}

/** The flame of a preset, unframed. */
function build(p: Preset): Flame {
  const rnd = randomFrom(seedOf(`form ${Math.round(p.form)}`));
  const parts = Array.from({ length: PARTS }, () => partOf(Array.from({ length: DRAWS }, rnd), p));
  // The Colours' Cover and Fire (65: family 4 and 5) colour over the bands' own hues: Spectrum's, the most apart.
  huesOf(p.family > 3.5 ? 0 : Math.round(p.family), p.hue, Array.from({ length: 2 + PARTS }, rnd)).forEach((h, i) => {
    parts[i].color = h.color;
    parts[i].colorReach = h.reach;
  });
  const f = Array.from({ length: 8 }, rnd);
  const lens = Math.round(p.final);
  const order = Math.max(1, Math.round(p.symmetry));
  const mirror = Math.round(p.mirror) === 1;
  const colours = { light: 0.55 + 0.3 * p.lightness, chroma: 0.06 + 0.2 * p.vividness };
  const final = lens > 0 ? finalOf(f, lens, { sides: p.lensSides ?? LENS_SIDES, depth: p.lensDepth ?? LENS_DEPTH }) : undefined;
  return withSymmetry({ xforms: parts, final, symmetry: { order, mirror }, colours, center: [0, 0], size: 1 }, order, mirror);
}

/** The flame of a preset, framed; or not (`framed` false), where whatever draws it frames it again anyway (the
 *  visuals do, as it moves: `measuring`), a sixth of a second on a slow tablet saved each time the look changes. */
export function flameOf(p: Preset, framed = true): Flame {
  const flame = build(p);
  const m = framed ? measure(flame) : null;
  if (m) {
    flame.center = m.center;
    flame.size = m.size;
  }
  return flame;
}

/** A track's preset (from its key's seed), or the dice's: drawn until its flame passes the check (`survey`), the best
 *  of 24 draws where none does. `fixed`: settings pinned, kept as they are (the check sees them as they are); `own`:
 *  the draw without them, what a pin let go of returns to. A third without symmetry, two fifths turned round (2 to 8
 *  arms), the rest mirrored too (1 to 6); a lens on three in ten (it warps the whole picture, at a third more of the GPU's
 *  time a step); the families as often as they came before. */
export function presetOf(seed: number, fixed: Partial<Preset> = {}): Drawn {
  const draws = drawing(seed, fixed);
  for (;;) {
    const next = draws.next();
    if (next.done) return next.value;
  }
}

export type Drawn = { preset: Preset; own: Preset };

/** `presetOf` a draw at a time: it yields after each one checked (each a few milliseconds on a slow tablet), so the
 *  draws can be spread over frames or idle time. */
export function* drawing(seed: number, fixed: Partial<Preset> = {}): Generator<void, Drawn> {
  const rnd = randomFrom(seed);
  const effects = effectsDrawn(seed);
  const facets = facetsDrawn(seed);
  let best: { preset: Preset; own: Preset; score: number } | null = null;
  for (let attempt = 0; attempt < 24; attempt++) {
    const kind = rnd();
    const family = rnd();
    const own: Preset = {
      form: Math.floor(rnd() * FORMS),
      detail: 0.15 + rnd() * 0.7,
      twist: 0.3 + rnd() * 0.7,
      warp: 0.55 + rnd() * 0.45,
      symmetry: kind < 0.35 ? 1 : kind < 0.75 ? 2 + Math.floor(rnd() * 7) : 1 + Math.floor(rnd() * 6),
      mirror: kind >= 0.75 ? 1 : 0,
      final: rnd() < 0.3 ? 1 + Math.floor(rnd() * DRAWN_FINALS) : 0,
      family: family < 0.4 ? 0 : family < 0.65 ? 1 : family < 0.85 ? 2 : 3,
      hue: rnd(),
      lightness: 0.23 + rnd() * 0.54,
      vividness: 0.3 + rnd() * 0.4,
      spread: 0.8 + rnd() * 0.5,
      purity: 0.6 + rnd() * 0.35,
      ...effects,
      ...facets,
      lensSides: LENS_SIDES,
      lensDepth: LENS_DEPTH,
    };
    const preset = { ...own, ...fixed };
    if (attempt > 0) yield;
    const look = survey(build(preset), seed + attempt);
    if (!look) continue;
    if (look.score > 0.5) return { preset, own };
    if (!best || look.score > best.score) best = { preset, own, score: look.score };
  }
  return best ?? { preset: { ...STANDBY, ...fixed }, own: STANDBY };
}

/** How well a look draws (0: nothing on the screen; above ½ a flame worth showing): the survey a look is checked by, its
 *  seed the roll's (the dice's macro rolls: engine/dice.ts). */
export const drawScore = (p: Preset, seed: number): number => survey(build(p), seed)?.score ?? 0;

/** The flame of a seed, as a track's preset makes it. */
export const flameFrom = (seed: number): Flame => flameOf(presetOf(seed).preset);

/** The visuals' Colours setting: the preset's own (its family), the playing track's cover's, or a fire's. */
export type ColourMode = 'rainbow' | 'cover' | 'fire';

/** The flame's parts coloured by `mode`; its shape as it is. Cover: the cover's colours (`cover`, OKLab, darkest first:
 *  palette.ts) given to the bands in order, the darkest to the sub and the lightest to the air, their lightness brought
 *  into what shows on black (0.58 to 0.86, in the cover's order); the seed's own where there is no cover. Fire: from a
 *  deep red at the sub through orange to a yellow at the air, darker to lighter (the display adds a blackbody's heat
 *  where the points are dense: render.ts). */
export function coloured(flame: Flame, mode: ColourMode, cover: { L: number; a: number; b: number }[] | null): Flame {
  if (mode === 'rainbow' || (mode === 'cover' && !cover?.length)) return flame;
  let n = 0;
  const lights: number[] = [];
  const hues: { hue: number; chroma: number }[] = [];
  for (let i = 0; i < PARTS; i++) {
    if (mode === 'fire') {
      const k = i / (PARTS - 1);
      lights.push(0.56 + 0.3 * k);
      hues.push({ hue: 0.07 + 0.2 * k, chroma: 0.17 - 0.03 * k });
    } else {
      const c = cover![i % cover!.length];
      lights.push(0.58 + 0.28 * Math.min(1, Math.max(0, c.L)));
      // A third more chroma than the cover's own (up to 0.26): its colours are seen against each other, the flame's
      // against black and through each other, where they went pale (The Low End Theory's reds and greens, 2026-10-18).
      hues.push({ hue: (((Math.atan2(c.b, c.a) / (2 * Math.PI)) % 1) + 1) % 1, chroma: Math.min(0.26, 1.35 * Math.hypot(c.a, c.b)) });
    }
  }
  const chroma = Math.max(0.05, ...hues.map((h) => h.chroma));
  return {
    ...flame,
    xforms: flame.xforms.map((t) => {
      if (!t.turns) return t;
      const h = hues[n++ % PARTS];
      return { ...t, color: h.hue, colorReach: h.chroma / chroma };
    }),
    colours: { light: lights.reduce((a, b) => a + b, 0) / PARTS, chroma, lights },
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
      const shown = flame.final ? apply(flame.final, x, y, rnd) : p;
      if (Number.isFinite(shown[0]) && Number.isFinite(shown[1]) && Math.abs(shown[0]) < 1e4 && Math.abs(shown[1]) < 1e4) {
        xs.push(shown[0]);
        ys.push(shown[1]);
      }
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
  // Electric Sheep's flames leave dark around them: a flame filling more than 70 % of its frame scores less (and one
  // under 4 %, a point or a thread).
  const fits = coverage < 0.04 ? coverage / 0.04 : coverage > 0.7 ? Math.max(0, (0.95 - coverage) / 0.25) : 1;
  const structured = Math.min(1, unevenness / 0.5);
  // Haze stays as strict as it was: it is grain however long the flame is drawn.
  const clear = haze > 0.35 ? Math.max(0, (0.6 - haze) / 0.25) : 1;
  return { center: [cx, cy], size: size * 1.1, score: fits * structured * clear };
}

/** Where a flame's points fall now (as it is drawn: turned, evolved), on the CPU: the middle and half-size of the
 *  frame that holds 96 % of them, and how much of that frame's 32 x 32 cells they reach. The visuals measure their flame
 *  so twice a second and follow it slowly with their framing and exposure: a flame whose transforms turn passes through
 *  shapes three times as large as its own and back (measured: 0.63 to 3.1, 2026-10-18), which showed as flares the
 *  music had no part in. */
export function measure(flame: Flame, steps = 6000): Seen | null {
  const m = measuring(flame, steps, steps);
  for (;;) {
    const r = m.next();
    if (r.done) return r.value;
  }
}

export type Seen = { center: [number, number]; size: number; coverage: number };

/** `measure` a slice at a time: `slice` steps each time it is moved on, its value the measure once all `steps` are
 *  taken. All at once it was 28 ms of a tablet's processor (Helio G88, 2026-10-18), which made a frame late twice a
 *  second; the visuals take a slice a frame. */
export function* measuring(flame: Flame, steps = 6000, slice = 1500): Generator<void, Seen | null> {
  const rnd = randomFrom(0x51ced);
  const pick = picker(flame.xforms);
  const xs: number[] = [];
  const ys: number[] = [];
  let x = rnd() * 2 - 1;
  let y = rnd() * 2 - 1;
  for (let i = 0; i < steps; i++) {
    if (i > 0 && i % slice === 0) yield;
    const p = apply(flame.xforms[pick(rnd())], x, y, rnd);
    if (!Number.isFinite(p[0]) || !Number.isFinite(p[1]) || Math.abs(p[0]) > 1e4 || Math.abs(p[1]) > 1e4) {
      x = rnd() * 2 - 1;
      y = rnd() * 2 - 1;
      continue;
    }
    [x, y] = p;
    if (i >= 20) {
      const shown = flame.final ? apply(flame.final, x, y, rnd) : p;
      if (Number.isFinite(shown[0]) && Number.isFinite(shown[1]) && Math.abs(shown[0]) < 1e4 && Math.abs(shown[1]) < 1e4) {
        xs.push(shown[0]);
        ys.push(shown[1]);
      }
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

/** Where each band's part lies as drawn (its final transform taken, before the symmetry's copies carry it round): the
 *  middle of the points its transform takes, on the CPU over `steps` of the chaos game; null for a part with none.
 *  The fluid is pushed there (decision 0047). */
export function partsAt(flame: Flame, steps = 1200): ([number, number] | null)[] {
  const rnd = randomFrom(0x9a27);
  const pick = picker(flame.xforms);
  const sums = Array.from({ length: PARTS }, () => [0, 0, 0]);
  let x = rnd() * 2 - 1;
  let y = rnd() * 2 - 1;
  for (let i = 0; i < steps; i++) {
    const k = pick(rnd());
    const p = apply(flame.xforms[k], x, y, rnd);
    if (!Number.isFinite(p[0]) || !Number.isFinite(p[1]) || Math.abs(p[0]) > 1e4 || Math.abs(p[1]) > 1e4) {
      x = rnd() * 2 - 1;
      y = rnd() * 2 - 1;
      continue;
    }
    [x, y] = p;
    if (i < 20 || k >= PARTS || !flame.xforms[k].turns) continue;
    const shown = flame.final ? apply(flame.final, x, y, rnd) : p;
    if (!Number.isFinite(shown[0]) || !Number.isFinite(shown[1]) || Math.abs(shown[0]) > 1e4 || Math.abs(shown[1]) > 1e4) continue;
    sums[k][0] += shown[0];
    sums[k][1] += shown[1];
    sums[k][2]++;
  }
  return sums.map(([sx, sy, n]) => (n > 0 ? [sx / n, sy / n] : null));
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
      case VARIATIONS.cratarr: [vx, vy] = markOf(x, y, r); break;
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
      case VARIATIONS.bent: vx = x < 0 ? 2 * x : x; vy = y < 0 ? y / 2 : y; break;
      case VARIATIONS.exponential: { const k = Math.exp(x - 1); vx = k * Math.cos(Math.PI * y); vy = k * Math.sin(Math.PI * y); break; }
      case VARIATIONS.power: { const k = r ** Math.sin(theta); vx = k * Math.cos(theta); vy = k * Math.sin(theta); break; }
      case VARIATIONS.cosine: vx = Math.cos(Math.PI * x) * Math.cosh(y); vy = -Math.sin(Math.PI * x) * Math.sinh(y); break;
      case VARIATIONS.cylinder: vx = Math.sin(x); vy = y; break;
      case VARIATIONS.tangent: vx = Math.sin(x) / Math.cos(y); vy = Math.tan(y); break;
      case VARIATIONS.cross: { const k = 1 / Math.abs(x * x - y * y + 1e-12); vx = k * x; vy = k * y; break; }
      case VARIATIONS.waves: vx = x + b * Math.sin(y / (c * c + 1e-6)); vy = y + e * Math.sin(x / (f * f + 1e-6)); break;
      case VARIATIONS.popcorn: vx = x + c * Math.sin(Math.tan(3 * y)); vy = y + f * Math.sin(Math.tan(3 * x)); break;
      case VARIATIONS.rings: {
        const k = c * c + 1e-6;
        const m = ((((r + k) % (2 * k)) + 2 * k) % (2 * k)) - k + r * (1 - k);
        vx = m * Math.cos(theta);
        vy = m * Math.sin(theta);
        break;
      }
      case VARIATIONS.fan: {
        const k = Math.PI * (c * c + 1e-6);
        const a2 = ((((theta + f) % k) + k) % k) > k / 2 ? theta - k / 2 : theta + k / 2;
        vx = r * Math.cos(a2);
        vy = r * Math.sin(a2);
        break;
      }
      case VARIATIONS.noise: { const p1 = rnd(); const p2 = 2 * Math.PI * rnd(); vx = p1 * x * Math.cos(p2); vy = p1 * y * Math.sin(p2); break; }
      case VARIATIONS.blur: { const p1 = rnd(); const p2 = 2 * Math.PI * rnd(); vx = p1 * Math.cos(p2); vy = p1 * Math.sin(p2); break; }
      case VARIATIONS.arch: { const a2 = rnd() * Math.PI; vx = Math.sin(a2); vy = (Math.sin(a2) * Math.sin(a2)) / Math.cos(a2); break; }
      case VARIATIONS.square: vx = rnd() - 0.5; vy = rnd() - 0.5; break;
      case VARIATIONS.rays: { const k = Math.tan(rnd() * Math.PI) / (r2 + 1e-6); vx = k * Math.cos(x); vy = k * Math.sin(y); break; }
      case VARIATIONS.blade: { const a2 = rnd() * r; vx = x * (Math.cos(a2) + Math.sin(a2)); vy = x * (Math.cos(a2) - Math.sin(a2)); break; }
      case VARIATIONS.secant2: { const cr = Math.cos(r); vx = x; vy = 1 / cr + (cr < 0 ? 1 : -1); break; }
      case VARIATIONS.twintrian: {
        const a2 = rnd() * r;
        const s = Math.sin(a2);
        let k = Math.log10(s * s + 1e-12) + Math.cos(a2);
        if (!Number.isFinite(k)) k = -30;
        vx = x * k;
        vy = x * (k - s * Math.PI);
        break;
      }
      default: {
        // The lens's warps (warps.ts), and its shapes (shape-set.ts: the point sent into the shape evenly).
        const warped = warpOf(v, x, y, r2, r, theta, rnd);
        if (warped) [vx, vy] = warped;
        else if (v >= LENS_SHAPE_FIRST) [vx, vy] = lensInto(LENS_SHAPES[v - LENS_SHAPE_FIRST], x, y, r, t.shape);
      }
    }
    ox += w * vx;
    oy += w * vy;
  }
  if (t.post) {
    const [pa, pb, pc, pd, pe, pf] = t.post;
    return [pa * ox + pb * oy + pc, pd * ox + pe * oy + pf];
  }
  return [ox, oy];
}

// --- Animation and blending ---

/** The variation each band's part evolves toward (decision 0046, amended; a flame's first six transforms are the
 *  bands'): round and heavy for the sub (bubble), a bulge for the bass (eyefish), a twist for the low mids (swirl), a
 *  bend for the mids (horseshoe), a wave across for the presence (cylinder), a fine ripple for the air (sinusoidal). Six until
 *  2026-10-18: the three bands' three left the upper three parts evolving toward none (linear). */
const BAND_VARIATIONS: Variation[] = [
  VARIATIONS.bubble,
  VARIATIONS.eyefish,
  VARIATIONS.swirl,
  VARIATIONS.horseshoe,
  VARIATIONS.cylinder,
  VARIATIONS.sinusoidal,
];

/** How a band's part has evolved: turned by `turn` (radians, as Electric Sheep's loops turn the linear part), that
 *  part scaled by `size`, `morph` (0..1) of its variations given over to its band's own, its hue moved by `hue` (a
 *  turn of the wheel), and its offset from the middle scaled by `spread` (1: as it is). */
/** `weight`: its share of the points against its own (Presence, 65: 1 as drawn). */
export type Part = { turn: number; size: number; morph: number; hue: number; spread?: number; weight?: number };

/** The most a transform's linear part may stretch anything (its larger singular value): under 1, every transform pulls
 *  points together, so turning and growing never carry the flame through a shape that flies apart over the screen. */
const STRETCH = 0.95;

/** The flame with each band's transform evolved by its part (the symmetry's rotations stay as they are). `purity` (0..1):
 *  how far each band's transform takes a point's colour all the way to its own (1: every point its last transform's
 *  colour, so each band's part is its colour alone). `spread`: the parts' offsets from the middle scaled (1: the seed's
 *  own), drawing them together into one shape (under 1) or apart into clusters (over 1). */
export function evolved(flame: Flame, parts: Part[], purity = 0, spread = 1): Flame {
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
      // Spread: the parts' offsets scaled apart (separate clusters) or together (one shape).
      const out = spread * (p.spread ?? 1);
      return {
        ...t,
        affine: [linear[0], linear[1], c * out, linear[2], linear[3], f * out],
        variations,
        color: (((t.color + p.hue) % 1) + 1) % 1,
        colorSpeed: t.colorSpeed + purity * (1 - t.colorSpeed),
        weight: t.weight * (p.weight ?? 1),
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
      colorReach: mix(p.colorReach ?? 1, q.colorReach ?? 1, k),
      post: p.post || q.post ? blendAffine(p.post ?? IDENTITY, q.post ?? IDENTITY, k) : undefined,
      turns: k < 0.5 ? p.turns : q.turns,
    });
  }
  // The final transforms blend as the others do, the plain picture standing in for a missing one.
  const plain: Xform = { affine: IDENTITY, variations: new Map([[VARIATIONS.linear, 1]]), weight: 0, color: 0, colorSpeed: 0, turns: false };
  const final =
    from.final || to.final
      ? (() => {
          const p = from.final ?? plain;
          const q = to.final ?? plain;
          const variations = new Map<Variation, number>();
          for (const v of new Set([...p.variations.keys(), ...q.variations.keys()])) variations.set(v, mix(p.variations.get(v) ?? 0, q.variations.get(v) ?? 0, k));
          const shape = p.shape && q.shape ? { sides: mix(p.shape.sides, q.shape.sides, k), depth: mix(p.shape.depth, q.shape.depth, k) } : (q.shape ?? p.shape);
          return { ...plain, affine: blendAffine(p.affine, q.affine, k), variations, ...(shape ? { shape } : {}) };
        })()
      : undefined;
  return {
    xforms,
    final,
    symmetry: k < 0.5 ? from.symmetry : to.symmetry,
    colours: {
      light: mix(from.colours.light, to.colours.light, k),
      chroma: mix(from.colours.chroma, to.colours.chroma, k),
      lights:
        from.colours.lights || to.colours.lights
          ? Array.from({ length: PARTS }, (_, i) => mix(from.colours.lights?.[i] ?? from.colours.light, to.colours.lights?.[i] ?? to.colours.light, k))
          : undefined,
    },
    center: [mix(from.center[0], to.center[0], k), mix(from.center[1], to.center[1], k)],
    size: Math.exp(mix(Math.log(from.size), Math.log(to.size), k)),
  };
}

const mix = (a: number, b: number, k: number) => a + (b - a) * k;
const IDENTITY: Affine = [1, 0, 0, 0, 1, 0];
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
