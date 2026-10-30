// The tape deck's drawing (the owner, 2026-10-27: during an effect, in the player's row under the title): two reels
// whose packs follow the place in the track, the tape off their bottoms round a low roller each, up to two inner
// wheels that hold it straight and level through the head's two hills. Every wheel bends the tape inwards (the part on
// the inside of the turn); the layout was tested so at 51 places in a track (no tape through a wheel, the run through
// the head level). Screen coordinates (y down), a 320 × 47 box; `s` +1 = the tape goes round a part clockwise as seen.

export const HUB = 5;
export const MAXR = 19;
export const REEL_L: readonly [number, number] = [70, 21];
export const REEL_R: readonly [number, number] = [250, 21];
/** The wheels between the reels, in the tape's order, and the head: its two hills either side of the tape's line. */
export const WHEELS: readonly { c: readonly [number, number]; r: number; s: 1 | -1 }[] = [
  { c: [106, 39], r: 3, s: -1 },
  { c: [139, 23.5], r: 2.5, s: 1 },
  { c: [181, 23.5], r: 2.5, s: 1 },
  { c: [214, 39], r: 3, s: -1 },
];
export const HEAD = { x: 160, y: 21, half: 12, height: 5, gap: 1.6 };

type Part = { c: readonly [number, number]; r: number; s: number };
type Pt = [number, number];

/** A pack's radius for the place played (left: what is played, right: what is left). */
export const packs = (place: number): [number, number] => [HUB + 2 + (MAXR - HUB - 2) * Math.sqrt(place), HUB + 2 + (MAXR - HUB - 2) * Math.sqrt(1 - place)];

/** The tangent from part a to part b for their senses: its two points (on a, then on b), or null. */
function tangent(a: Part, b: Part): [Pt, Pt] | null {
  const [dx, dy] = [b.c[0] - a.c[0], b.c[1] - a.c[1]];
  const L = Math.hypot(dx, dy);
  // Signed radii: a common tangent's normal n has n·(cb − ca) = ρa − ρb, the points at c + ρ·n.
  const [ra, rb] = [a.s * a.r, b.s * b.r];
  const cos = (ra - rb) / L;
  if (Math.abs(cos) > 1) return null;
  const th = Math.atan2(dy, dx), al = Math.acos(cos);
  for (const ang of [th + al, th - al]) {
    const n = [Math.cos(ang), Math.sin(ang)];
    const p: Pt = [a.c[0] + ra * n[0], a.c[1] + ra * n[1]];
    const q: Pt = [b.c[0] + rb * n[0], b.c[1] + rb * n[1]];
    // Leaving a in its sense: clockwise (as seen) at a point u from the centre runs along (−u.y, u.x).
    const u = [(p[0] - a.c[0]) / a.r, (p[1] - a.c[1]) / a.r];
    if (a.s * ((q[0] - p[0]) * -u[1] + (q[1] - p[1]) * u[0]) > 0) return [p, q];
  }
  return null;
}

/** The tape's SVG path for a place in the track. */
export function tapePath(place: number): string {
  const [rl, rr] = packs(place);
  const parts: Part[] = [{ c: REEL_L, r: rl, s: -1 }, ...WHEELS, { c: REEL_R, r: rr, s: -1 }];
  const legs: [Pt, Pt][] = [];
  for (let i = 0; i + 1 < parts.length; i++) {
    const leg = tangent(parts[i], parts[i + 1]);
    if (!leg) return '';
    legs.push(leg);
  }
  const f = (p: Pt) => `${p[0].toFixed(2)} ${p[1].toFixed(2)}`;
  let d = `M${f(legs[0][0])}L${f(legs[0][1])}`;
  for (let i = 1; i < legs.length; i++) {
    const part = parts[i];
    const [, inB] = legs[i - 1];
    const [outA, outB] = legs[i];
    const a0 = Math.atan2(inB[1] - part.c[1], inB[0] - part.c[0]), a1 = Math.atan2(outA[1] - part.c[1], outA[0] - part.c[0]);
    let sweep = (a1 - a0) * part.s;
    while (sweep < 0) sweep += 2 * Math.PI;
    d += `A${part.r} ${part.r} 0 ${sweep > Math.PI ? 1 : 0} ${part.s > 0 ? 1 : 0} ${f(outA)}L${f(outB)}`;
  }
  return d;
}

/** A head hill's path: thin and long, its point towards the tape (`dir` −1 above it, +1 below). */
export function hill(dir: 1 | -1): string {
  const h = HEAD;
  const apex = h.y + dir * h.gap, base = h.y + dir * (h.gap + h.height);
  return `M${h.x - h.half} ${base}C${h.x - h.half * 0.45} ${base} ${h.x - h.half * 0.4} ${apex} ${h.x} ${apex}C${h.x + h.half * 0.4} ${apex} ${h.x + h.half * 0.45} ${base} ${h.x + h.half} ${base}Z`;
}
