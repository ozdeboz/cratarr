// The mark in motion: Cratarr's loading sign, as the owner tuned it (handoffs/cratarr-mark-motion.html). A load is the
// Reveal (nothing to the whole mark), then loop variations in shuffle-bag order until the load is done, then the Vanish
// after the playing variation ends. Every loop variation starts and ends on the whole mark. Pure: frames are computed
// from a variation and a time; lib/MarkLoader.svelte draws them.
//
// Geometry: tools/brand/record.py's `full` (the record's square 30..78, centre 54,54, the mouth to the right).

const CX = 54;
const CY = 54;
const MOUTH = 60;
/** Inside out: hub, label, three grooves. */
const RINGS = [
  { rIn: 0, rOut: 3 },
  { rIn: 8, rOut: 12.4 },
  { rIn: 13.6, rOut: 16.4 },
  { rIn: 17.6, rOut: 20.4 },
  { rIn: 21.6, rOut: 24 },
];
const N = RINGS.length;
/** The three lines in the mouth, at radius 13, 17 and 21; line j sits just outside ring j+1. */
const LINE_W = 1.2;
const REST_LINES = [13, 17, 21];
/** The lines show only between these edges: one leaves past the rim as a new one slides out from under the inner
 *  edge, which is the outer side of the line waiting at radius 9 (so at rest three lines show). */
export const LINE_EDGE_IN = REST_LINES[0] - 4 + LINE_W / 2 + 0.02;
export const LINE_EDGE_OUT = 24;
export const CENTRE = { x: CX, y: CY };

/** Each ring's length (ms), the stagger between rings, the rest between variations: the owner's choice. */
const D = 1000;
const S = 150;
export const GAP = 500;
const HOLD = D * 0.3;

const f = (n: number) => n.toFixed(4);
const pt = (r: number, deg: number) => {
  const a = (deg * Math.PI) / 180;
  return `${f(CX + r * Math.cos(a))},${f(CY + r * Math.sin(a))}`;
};

/** A ring's piece from angle a0 to a1 (clockwise on screen). Every arc is drawn as two halves through its middle, so no
 *  arc spans more than 180°: one arc whose start and end nearly meet is dropped by browsers, and a ring flashed away
 *  as a mouth closed. A piece within 0.1° of the full circle is the whole ring. */
function sector(rIn: number, rOut: number, a0: number, a1: number): string {
  const span = a1 - a0;
  if (span <= 0.05 || rOut - rIn <= 0.005) return '';
  if (span >= 359.9) {
    const ring = (r: number, s: number) => `M${pt(r, 0)}A${r},${r} 0 0 ${s} ${pt(r, 180)}A${r},${r} 0 0 ${s} ${pt(r, 0)}Z`;
    return rIn > 0 ? ring(rOut, 1) + ring(rIn, 0) : ring(rOut, 1);
  }
  const am = (a0 + a1) / 2;
  const outer = `A${rOut},${rOut} 0 0 1 ${pt(rOut, am)}A${rOut},${rOut} 0 0 1 ${pt(rOut, a1)}`;
  if (rIn <= 0) return `M${CX},${CY}L${pt(rOut, a0)}${outer}Z`;
  return `M${pt(rOut, a0)}${outer}L${pt(rIn, a1)}A${rIn},${rIn} 0 0 0 ${pt(rIn, am)}A${rIn},${rIn} 0 0 0 ${pt(rIn, a0)}Z`;
}

const ease = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
const clamp = (x: number) => (x < 0 ? 0 : x > 1 ? 1 : x);
const e = (x: number) => ease(clamp(x));

type Line = { r: number; w: number; o: number; of: number; m: number; c: number };
/** One frame: every ring's mouth (degrees) and turn, every line's radius, width and opacity. */
type State = { m: number[]; c: number[]; lines: Line[] };

function full(): State {
  return {
    m: Array(N).fill(MOUTH),
    c: Array(N).fill(0),
    lines: REST_LINES.map((r, j) => ({ r, w: LINE_W, o: 1, of: j + 1, m: MOUTH, c: 0 })),
  };
}
function none(): State {
  const st = full();
  st.m.fill(360);
  for (const l of st.lines) l.o = 0;
  return st;
}
const follow = (st: State) => {
  for (const l of st.lines) {
    l.m = st.m[l.of];
    l.c = st.c[l.of];
  }
  return st;
};

// The two halves the bookends share: winding in (outer ring first) and growing back (hub first). The lines fade early
// on the way in and late on the way out, else they would fill the gaps into a solid disc.
const windT = () => (N - 1) * S + D;
function windIn(st: State, t: number): State {
  for (let i = 0; i < N; i++) st.m[i] = MOUTH + (360 - MOUTH) * ease(clamp((t - (N - 1 - i) * S) / D));
  for (const l of st.lines) {
    l.m = MOUTH;
    l.c = 0;
    l.o = 1 - e(clamp((t - (N - 1 - l.of) * S) / D) * 2.5);
  }
  return st;
}
function growOut(st: State, t: number): State {
  for (let i = 0; i < N; i++) st.m[i] = 360 - (360 - MOUTH) * ease(clamp((t - i * S) / D));
  for (const l of st.lines) {
    l.m = MOUTH;
    l.c = 0;
    l.o = e((clamp((t - l.of * S) / D) - 0.6) * 2.5);
  }
  return st;
}

type Variant = { T: number; at: (t: number) => State };
const RADIATE_STEPS = 2;
const radiateStep = REST_LINES.length * S + D;
export const VARIANTS: Record<'reveal' | 'vanish' | 'mouth' | 'turn' | 'radiate' | 'blink', Variant> = {
  reveal: { T: windT(), at: (t) => growOut(full(), t) },
  vanish: { T: windT(), at: (t) => windIn(full(), t) },
  // Close mouth: the mouths shut hub first, a hold, then open outer ring first; each line follows the ring inside it.
  mouth: {
    T: 2 * windT() + HOLD,
    at(t) {
      const st = full();
      const b0 = windT() + HOLD;
      for (let i = 0; i < N; i++) st.m[i] = MOUTH * (1 - e((t - i * S) / D) + e((t - b0 - (N - 1 - i) * S) / D));
      return follow(st);
    },
  },
  // Turn: each ring one full turn, hub first.
  turn: {
    T: (N - 1) * S + D * 1.6,
    at(t) {
      const st = full();
      for (let i = 0; i < N; i++) st.c[i] = 360 * e((t - i * S) / (D * 1.6));
      return follow(st);
    },
  },
  // Radiate: the lines step outward by 4, twice, the outer one first; one more waits under the inner edge.
  radiate: {
    T: RADIATE_STEPS * radiateStep,
    at(t) {
      const st = full();
      const u = t - Math.min(RADIATE_STEPS - 1, Math.floor(t / radiateStep)) * radiateStep;
      const bases = [REST_LINES[0] - 4, ...REST_LINES];
      st.lines = bases.map((b, k) => ({ r: b + 4 * e((u - (bases.length - 1 - k) * S) / D), w: LINE_W, o: 1, of: 0, m: MOUTH, c: 0 }));
      return st;
    },
  },
  // Vanish and reveal: the Vanish half, a hold, the Reveal half.
  blink: {
    T: 2 * windT() + HOLD,
    at(t) {
      const b0 = windT() + HOLD;
      return t < b0 ? windIn(full(), t) : growOut(full(), t - b0);
    },
  },
};
export type VariantName = keyof typeof VARIANTS;
const POOL: VariantName[] = ['mouth', 'turn', 'radiate', 'blink'];

/** The paths of a frame: the five rings, and up to four lines with their opacity. */
export type Paths = { rings: string[]; lines: [string, number][] };
export function paths(st: State): Paths {
  return {
    rings: RINGS.map((r, i) => sector(r.rIn, r.rOut, st.c[i] + st.m[i] / 2, st.c[i] + 360 - st.m[i] / 2)),
    lines: [0, 1, 2, 3].map((k): [string, number] => {
      const l = st.lines[k];
      if (!l || l.o <= 0.001) return ['', 0];
      return [sector(l.r - l.w / 2, l.r + l.w / 2, l.c - l.m / 2, l.c + l.m / 2), Math.min(1, l.o)];
    }),
  };
}
export const WHOLE = paths(full());
export const NOTHING = paths(none());

/** What plays next: every loop variation once per round, a round never starting with the one that just played. */
export function shuffleBag(): () => VariantName {
  let bag: VariantName[] = [];
  let last: VariantName | null = null;
  return () => {
    if (!bag.length) {
      bag = POOL.slice();
      for (let i = bag.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [bag[i], bag[j]] = [bag[j], bag[i]];
      }
      if (bag[0] === last) [bag[0], bag[bag.length - 1]] = [bag[bag.length - 1], bag[0]];
    }
    return (last = bag.shift()!);
  };
}
