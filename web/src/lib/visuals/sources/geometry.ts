// The Shapes and Waves sources' geometry (decision 0056, step 4), pure so Node can test it; the shaders draw exactly
// this. Units: half the target's shorter side is 1, y up, the middle at 0.

export const TAU = Math.PI * 2;
const mod = (x: number, m: number) => x - m * Math.floor(x / m);

// ---- Polygons of any number of sides ----------------------------------------------------------------------------

/** A regular polygon's edge at a point (r, θ) from its middle, its first corner at θ = 0 and its corners `radius` out:
 *  how far past the nearest side's line (negative inside). */
export function polygonEdge(r: number, theta: number, sides: number, radius: number): number {
  const s = TAU / sides;
  const b = mod(theta, s) - s / 2;
  return r * Math.cos(b) - radius * Math.cos(s / 2);
}

/** The whole polygon under a number of sides (at least 3) and how far towards the next one it is. */
export function sidesOf(sides: number): [number, number] {
  const n = Math.max(3, sides);
  const n0 = Math.floor(n);
  return [n0, n - n0];
}

/** A fractional number of sides: the two whole polygons' edges mixed by the fraction (the shader's `edge`). At many
 *  sides both are nearly the circle. */
export function morphEdge(r: number, theta: number, sides: number, radius: number): number {
  const [n0, f] = sidesOf(sides);
  return (1 - f) * polygonEdge(r, theta, n0, radius) + f * polygonEdge(r, theta, n0 + 1, radius);
}

/** Where the outline of `morphEdge` is at θ: it is linear in r, so its zero is exact. Never past `radius`. */
export function outlineRadius(theta: number, sides: number, radius: number): number {
  const [n0, f] = sidesOf(sides);
  const s0 = TAU / n0;
  const s1 = TAU / (n0 + 1);
  const b0 = mod(theta, s0) - s0 / 2;
  const b1 = mod(theta, s1) - s1 / 2;
  return (radius * ((1 - f) * Math.cos(s0 / 2) + f * Math.cos(s1 / 2))) / ((1 - f) * Math.cos(b0) + f * Math.cos(b1));
}

/** The outline as `steps` points, turned by `rotation` (radians). */
export function outline(sides: number, radius: number, rotation: number, steps: number): [number, number][] {
  const out: [number, number][] = [];
  for (let i = 0; i < steps; i++) {
    const t = (TAU * i) / steps;
    const r = outlineRadius(t, sides, radius);
    out.push([r * Math.cos(t + rotation), r * Math.sin(t + rotation)]);
  }
  return out;
}

/** HSV (each 0..1, hue around) as RGB. */
export function hsv(h: number, s: number, v: number): [number, number, number] {
  const c = (o: number) => {
    const k = mod(h * 6 + o, 6);
    return v * (1 - s + s * (1 - Math.min(1, Math.max(0, Math.min(k, 4 - k)))));
  };
  return [c(5), c(3), c(1)];
}

// ---- Shapes -----------------------------------------------------------------------------------------------------

export const MOST_SHAPES = 32;
/** A shape as drawn: its middle, its corners' radius, its turn (radians), its colour (straight) and alpha. */
export const SHAPE_FLOATS = 8;
/** The clocks the rates move (radians, integrated by dt, never rate × time). */
export type ShapeClocks = { orbit: number; spin: number };

/** The shapes this frame into `out`, `SHAPE_FLOATS` each; how many. They sit on a ring about (x, y), spaced by the
 *  count itself so a fractional count slides them apart while the last one fades in. `across` and `up` are the
 *  target's half sides in units. */
export function shapeLayout(k: Record<string, number>, clocks: ShapeClocks, across: number, up: number, out: Float32Array): number {
  const count = Math.min(MOST_SHAPES, Math.max(0, k.count));
  const drawn = Math.ceil(count);
  if (drawn === 0 || k.opacity <= 0) return 0;
  const step = 1 / Math.max(count, 1);
  let n = 0;
  for (let i = 0; i < drawn; i++) {
    const alpha = k.opacity * Math.min(1, count - i);
    if (alpha <= 0) continue;
    const at = clocks.orbit + TAU * i * step;
    // White where the shapes have no colour of their own (Irregular shapes: a Tint's after them).
    const [r, g, b] = k.hue === undefined ? [1, 1, 1] : hsv(k.hue + k.hueSpread * i * step, k.saturation, 1);
    const o = n * SHAPE_FLOATS;
    out[o] = k.x * across + k.spread * Math.cos(at);
    out[o + 1] = k.y * up + k.spread * Math.sin(at);
    out[o + 2] = k.radius * Math.max(0, 1 - k.taper * i * step);
    out[o + 3] = TAU * (k.rotation + k.twist * i) + clocks.spin;
    out[o + 4] = r;
    out[o + 5] = g;
    out[o + 6] = b;
    out[o + 7] = alpha;
    n++;
  }
  return n;
}

// ---- Waves ------------------------------------------------------------------------------------------------------

export const WAVE_POINTS = 256;
/** A segment as drawn: its ends A and B, and its alpha. */
export const SEGMENT_FLOATS = 5;
export const BANDS = 6;

/** A waveform of any length resampled to `out`'s, end to end. */
export function resample(src: ArrayLike<number>, out: Float32Array) {
  const n = out.length;
  const last = src.length - 1;
  for (let i = 0; i < n; i++) {
    const x = (i * last) / Math.max(1, n - 1);
    const j = Math.min(last - 1, Math.floor(x));
    const f = x - j;
    const v = (1 - f) * src[j] + f * src[j + 1];
    out[i] = Number.isFinite(v) ? Math.max(-1, Math.min(1, v)) : 0;
  }
}

/** Each band's whole number of cycles across the wave (so a circle closes), and how fast it drifts (cycles a second). */
const SYNTH_CYCLES = [1, 2, 3, 5, 8, 13];
const SYNTH_DRIFT = [0.11, 0.17, 0.23, 0.31, 0.43, 0.59];

/** A wave made of the bands' levels (1 = typical) where there is no sound to draw: a sine a band, the low ones wider
 *  and stronger, mixed so a typical moment stays within −1..1. */
export function synthWave(levels: ArrayLike<number>, time: number, out: Float32Array) {
  const n = out.length;
  out.fill(0);
  let total = 0;
  for (let b = 0; b < BANDS; b++) {
    const w = 1 / (1 + 0.4 * b);
    total += w;
    const l = levels[b];
    const a = w * (Number.isFinite(l) ? Math.max(0, Math.min(3, l)) : 0);
    if (a === 0) continue;
    const phase = TAU * SYNTH_DRIFT[b] * time * (b % 2 ? -1 : 1);
    for (let i = 0; i < n; i++) out[i] += a * Math.sin((TAU * SYNTH_CYCLES[b] * i) / n + phase);
  }
  for (let i = 0; i < n; i++) out[i] = Math.max(-1, Math.min(1, out[i] / total));
}

/** Smoothed forwards and back (no lag), by up to most of each sample. */
export function smooth(v: Float32Array, amount: number) {
  const k = 0.92 * Math.max(0, Math.min(1, amount));
  if (k === 0) return;
  for (let i = 1; i < v.length; i++) v[i] = k * v[i - 1] + (1 - k) * v[i];
  for (let i = v.length - 2; i >= 0; i--) v[i] = k * v[i + 1] + (1 - k) * v[i];
}

/** The wave's last eighth eased into its first sample, by `amount` (the circle's share), so a circle closes. */
export function closeSeam(v: Float32Array, amount: number) {
  const n = v.length;
  const m = Math.max(1, Math.floor(n / 8));
  for (let i = n - m; i < n; i++) {
    const t = (amount * (i - (n - m) + 1)) / m;
    v[i] = (1 - t) * v[i] + t * v[0];
  }
}

/** The wave's segments into `out`, `SEGMENT_FLOATS` each; how many. Three shapes, mixed point by point, never
 *  switched: a line across (`length` of the width), a circle (`radius`, the wave pushing it out and in) and the radial
 *  (a spoke a sample, out from the circle by the sample). Point n is the line's last again and the circle's first, so
 *  the last segment is nothing of the line and closes the circle; it shows as much as the circle or the radial does. */
export function waveSegments(v: Float32Array, k: Record<string, number>, spin: number, across: number, up: number, out: Float32Array): number {
  const n = v.length;
  const turn = TAU * k.rotation + spin;
  const [ct, st] = [Math.cos(turn), Math.sin(turn)];
  const [cx, cy] = [k.x * across, k.y * up];
  const { circle: c, radial: r, radius, amplitude: amp } = k;
  const pointX = (i: number) => {
    const t = (TAU * i) / n;
    return (1 - c) * ((Math.min(i, n - 1) / (n - 1)) * 2 - 1) * across * k.length + c * (radius + v[i % n] * amp) * Math.cos(t);
  };
  const pointY = (i: number) => {
    const t = (TAU * i) / n;
    return (1 - c) * v[Math.min(i, n - 1)] * amp + c * (radius + v[i % n] * amp) * Math.sin(t);
  };
  for (let i = 0; i < n; i++) {
    const t = (TAU * i) / n;
    const [dx, dy] = [Math.cos(t), Math.sin(t)];
    const outer = radius + v[i] * amp;
    const ax = (1 - r) * pointX(i) + r * radius * dx;
    const ay = (1 - r) * pointY(i) + r * radius * dy;
    const bx = (1 - r) * pointX(i + 1) + r * outer * dx;
    const by = (1 - r) * pointY(i + 1) + r * outer * dy;
    const o = i * SEGMENT_FLOATS;
    out[o] = cx + ct * ax - st * ay;
    out[o + 1] = cy + st * ax + ct * ay;
    out[o + 2] = cx + ct * bx - st * by;
    out[o + 3] = cy + st * bx + ct * by;
    out[o + 4] = i === n - 1 ? c + (1 - c) * r : 1;
  }
  return n;
}
