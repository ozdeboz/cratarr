// The effects stage (decision 0050; the owner: "the Hydra effect layer"): effects after Hydra (hydra-synth, AGPL-3.0
// like Cratarr), ported into the visuals' own shaders rather than embedded, so one renderer, one GL context, and the
// flash meter and the governor see everything.
//
// Two slots:
// - the picture: after the show pass, on the finished picture: shapes (where each pixel reads from), then one read of
//   the picture, then colours, then echoes (the last finished picture blended back in, as Hydra's src(o0));
// - the loop: shapes only, on where the light kept from frame to frame is read back (the fade's, the tunnel's, the
//   fluid's), so the flame's own trails are bent, turned, folded as they fade.
//
// A chain is a few effects' ids with their amounts and knobs; it is compiled into one shader a slot, rebuilt only when
// its ids change: the amounts and knobs are uniforms. Each effect says what it adds to the meters (meters.ts), so the
// governor holds it back as it holds the flame back. Pure: no GL here.

import type { Levers } from './governor';
import type { VisualSettings } from './settings.svelte';

/** The engines (the owner, 65: "a kaleidoscope engine, exyedo engine, and maybe one more? And then have multiple
 *  subeffects"): the picture's three slots, run in this order, each one of its sub-effects at a time. Kaleidoscope:
 *  symmetry and tiling; EYEDO (after EYESY): on the beat and in rhythm; Hydra: flow, feedback and colour. */
export type Engine = 'kaleidoscope' | 'eyedo' | 'hydra';
export const ENGINES: readonly { key: Engine; label: string }[] = [
  { key: 'kaleidoscope', label: 'Kaleidoscope' },
  { key: 'eyedo', label: 'EYEDO' },
  { key: 'hydra', label: 'Hydra' },
];

/** The meters an effect adds to, each a second at its amount (the meters' own units). */
export type Report = Partial<Record<'motion' | 'shapeChurn' | 'colourChurn' | 'zoomPumping', number>>;

/** One of an effect's own controls (the owner: "one or two of the most relevant controls per chosen subeffect"): its
 *  label in the panel, and its value (0..1, the half way a middle setting) as the panel shows it. */
export type Knob = { label: string; text: (v: number) => string };

/** An effect's two knobs (0..1 each; one it has no knob for is ignored). */
export type Knobs = readonly [number, number];

export type Effect = {
  /** Stable, kept in settings: never reused. */
  id: number;
  /** The panel's label. */
  name: string;
  /** shape: `vec2 fx_<name>(vec2 p, float a, float t, vec2 k)`, where to read instead; colour: `vec3 fx_<name>(vec3 c,
   *  vec2 p, float a, float t, vec2 k)`; echo: `vec3 fx_<name>(vec3 c, vec2 p, float a, float t, vec2 k, sampler2D
   *  prev)` (sample it with `fx_prev(prev, q)`). p: centred, aspect-true (y −1..1, x ±aspect); a: its amount (0: none,
   *  it must change nothing); t: the track's own time (seconds); k: its knobs (0..1, the half way a middle setting). On the
   *  picture, `fx_beat()` gives the beat: x where in it (0..1), y how long since it fell (seconds; long ago where there
   *  is none), z its strength (0..1), w where in its bar (0..1). */
  kind: 'shape' | 'colour' | 'echo';
  /** Its engine (the picture's three, each one of its sub-effects at a time: `ENGINES`). */
  engine: Engine;
  /** Where it may go: the loop takes shapes only. */
  slots: readonly ('picture' | 'loop')[];
  /** Its own controls, at most two, in `k`'s order. */
  knobs: readonly Knob[];
  /** One GLSL ES 3.00 function `fx_<name>`, and helpers named `fx_<name>_…`. */
  glsl: string;
  report: (amount: number, k: Knobs) => Report;
};

const between = (lo: number, hi: number, v: number) => lo + (hi - lo) * v;
const pct = (v: number) => `${Math.round(v * 100)}%`;
const fixed = (lo: number, hi: number, digits: number, unit = '') => (v: number) => `${between(lo, hi, v).toFixed(digits)}${unit}`;
const signedPct = (v: number) => (Math.abs(v - 0.5) < 0.01 ? 'None' : `${v > 0.5 ? '+' : '−'}${Math.round(Math.abs(v - 0.5) * 200)}%`);

const HSV = (name: string) => `vec3 fx_${name}_hsv(vec3 c) {
  vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
  vec4 q = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
  vec4 r = mix(vec4(q.xyw, c.r), vec4(c.r, q.yzx), step(q.x, c.r));
  float d = r.x - min(r.w, r.y);
  return vec3(abs(r.z + (r.w - r.y) / (6.0 * d + 1e-10)), d / (r.x + 1e-10), r.x);
}
vec3 fx_${name}_rgb(vec3 c) {
  vec3 p = abs(fract(c.xxx + vec3(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
  return c.z * mix(vec3(1.0), clamp(p - 1.0, 0.0, 1.0), c.y);
}`;

export const EFFECTS: readonly Effect[] = [
  {
    // ported from hydra-synth (AGPL-3.0), kaleid
    id: 1,
    name: 'kaleid',
    engine: 'kaleidoscope',
    kind: 'shape',
    slots: ['picture', 'loop'],
    knobs: [
      { label: 'Sides', text: (v) => `${Math.floor(2 + v * 10)}` },
      { label: 'Spin', text: signedPct },
    ],
    glsl: `vec2 fx_kaleid(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // Its sides (2 to 12), turning either way (none at the half way); its amount how far the picture is folded into it.
  float n = floor(2.0 + k.x * 10.0);
  float r = length(p);
  float th = atan(p.y, p.x) + t * (k.y - 0.5) * 0.4;
  float side = 6.28318530718 / n;
  th = mod(th, side);
  th = min(th, side - th);
  return mix(p, r * vec2(cos(th), sin(th)), a);
}`,
    report: (a, k) => ({ motion: Math.abs(k[1] - 0.5) * 0.4 * a, shapeChurn: 0.1 * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), posterize
    id: 2,
    name: 'posterize',
    engine: 'hydra',
    kind: 'colour',
    slots: ['picture'],
    knobs: [{ label: 'Steps', text: (v) => `${Math.round(between(16, 2, v))}` }],
    glsl: `vec3 fx_posterize(vec3 c, vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return c;
  // From many steps (none to see) down to its own (16 to 2 a channel) at all of it.
  float steps = mix(64.0, mix(16.0, 2.0, k.x), a);
  return mix(c, floor(c * steps + 0.5) / steps, min(1.0, a * 2.0));
}`,
    report: () => ({}),
  },
  {
    // ported from hydra-synth (AGPL-3.0), repeat
    id: 3,
    name: 'repeat',
    engine: 'kaleidoscope',
    kind: 'shape',
    slots: ['picture'],
    knobs: [{ label: 'Tiles', text: fixed(1, 6, 1, '×') }],
    glsl: `float fx_repeat_tri(float x) { return abs(mod(x + 1.0, 4.0) - 2.0) - 1.0; }
vec2 fx_repeat(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // From one picture to six across, mirrored at each seam so none shows.
  float n = 1.0 + 5.0 * k.x;
  return mix(p, vec2(fx_repeat_tri(p.x * n), fx_repeat_tri(p.y * n)), a);
}`,
    report: (a) => ({ shapeChurn: 0.05 * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), pixelate
    id: 4,
    name: 'pixelate',
    engine: 'kaleidoscope',
    kind: 'shape',
    slots: ['picture', 'loop'],
    knobs: [{ label: 'Size', text: (v) => `${Math.round(between(64, 6, v))} cells` }],
    glsl: `vec2 fx_pixelate(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // From fine (two hundred cells across the height) to its own size (64 to 6) at all of it.
  float cells = 0.5 * mix(200.0, mix(64.0, 6.0, k.x), a * a);
  return (floor(p * cells) + 0.5) / cells;
}`,
    report: () => ({}),
  },
  {
    // ported from hydra-synth (AGPL-3.0), colorama
    id: 5,
    name: 'colorama',
    engine: 'hydra',
    kind: 'colour',
    slots: ['picture'],
    knobs: [
      { label: 'Spread', text: pct },
      { label: 'Cycle', text: pct },
    ],
    glsl: `${HSV('colorama')}
vec3 fx_colorama(vec3 c, vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return c;
  // Each colour's hue moved by its own brightness (its spread) and round with the track (up to a tenth of a turn a
  // second): the colours shift in bands.
  vec3 h = fx_colorama_hsv(c);
  h.x = fract(h.x + a * (1.2 * k.x * h.z + 0.1 * k.y * t));
  return fx_colorama_rgb(h);
}`,
    report: (a, k) => ({ colourChurn: 0.1 * k[1] * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), hue
    id: 6,
    name: 'hue',
    engine: 'hydra',
    kind: 'colour',
    slots: ['picture'],
    knobs: [],
    glsl: `${HSV('hue')}
vec3 fx_hue(vec3 c, vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return c;
  // Every colour turned round the wheel by as much as half a turn; held, not cycling.
  vec3 h = fx_hue_hsv(c);
  h.x = fract(h.x + 0.5 * a);
  return fx_hue_rgb(h);
}`,
    report: () => ({}),
  },
  {
    // ported from hydra-synth (AGPL-3.0), modulate (by noise)
    id: 7,
    name: 'ripple',
    engine: 'hydra',
    kind: 'shape',
    slots: ['picture', 'loop'],
    knobs: [
      { label: 'Scale', text: fixed(0.5, 5, 1) },
      { label: 'Speed', text: pct },
    ],
    glsl: `float fx_ripple_hash(vec2 q) { return fract(sin(dot(q, vec2(127.1, 311.7))) * 43758.5453); }
float fx_ripple_noise(vec2 q) {
  vec2 i = floor(q);
  vec2 f = fract(q);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(fx_ripple_hash(i), fx_ripple_hash(i + vec2(1.0, 0.0)), u.x), mix(fx_ripple_hash(i + vec2(0.0, 1.0)), fx_ripple_hash(i + vec2(1.0, 1.0)), u.x), u.y);
}
vec2 fx_ripple(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // Moved by a noise of its scale, drifting at its speed, as far as a tenth of the height at all of it.
  vec2 q = p * mix(0.5, 5.0, k.x) + vec2(0.0, 0.4 * k.y * t);
  vec2 n = vec2(fx_ripple_noise(q), fx_ripple_noise(q + vec2(5.2, 1.3))) - 0.5;
  return p + 0.2 * a * n;
}`,
    report: (a, k) => ({ shapeChurn: 0.3 * k[1] * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), modulate (by the visuals' own flow)
    id: 8,
    name: 'stream',
    engine: 'hydra',
    kind: 'shape',
    slots: ['picture', 'loop'],
    knobs: [{ label: 'Length', text: (v) => `${Math.round(between(0.05, 0.5, v) * 1000)} ms` }],
    glsl: `vec2 fx_stream(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // Read from upstream: the picture drawn along the fluid's flow (nothing where it does not flow), as much of it as
  // its length (a twentieth to a half of a second).
  return p - mix(0.05, 0.5, k.x) * a * fx_flow(p);
}`,
    report: (a, k) => ({ shapeChurn: 0.36 * between(0.05, 0.5, k[0]) * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), src(o0) fed back, scaled and turned
    id: 9,
    name: 'echo',
    engine: 'hydra',
    kind: 'echo',
    slots: ['picture'],
    knobs: [
      { label: 'Zoom', text: pct },
      { label: 'Turn', text: pct },
    ],
    glsl: `vec3 fx_echo(vec3 c, vec2 p, float a, float t, vec2 k, sampler2D prev) {
  if (a <= 0.0) return c;
  // The last picture, larger by its zoom (up to six in a hundred a frame) and turned by its turn (up to 0.012 of a
  // radian a frame), kept where it is brighter: trails that stream out and round.
  float s = 1.0 + 0.06 * k.x * a;
  float r = 0.012 * k.y * a;
  vec2 q = mat2(cos(r), -sin(r), sin(r), cos(r)) * p / s;
  return max(c, fx_prev(prev, q) * (0.75 + 0.2 * a));
}`,
    report: (a, k) => ({ motion: 0.7 * k[1] * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), rotate
    id: 10,
    name: 'rotate',
    engine: 'kaleidoscope',
    kind: 'shape',
    slots: ['picture', 'loop'],
    knobs: [
      { label: 'Speed', text: pct },
      { label: 'Way', text: (v) => (v < 0.5 ? 'Left' : 'Right') },
    ],
    glsl: `vec2 fx_rotate(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // The whole turning with the track, up to a tenth of a turn a second, its way left or right.
  float r = 0.628318 * k.x * a * t * (k.y < 0.5 ? 1.0 : -1.0);
  return mat2(cos(r), -sin(r), sin(r), cos(r)) * p;
}`,
    report: (a, k) => ({ motion: 0.628318 * k[0] * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), scroll
    id: 11,
    name: 'scroll',
    engine: 'kaleidoscope',
    kind: 'shape',
    slots: ['picture', 'loop'],
    knobs: [
      { label: 'Speed', text: pct },
      { label: 'Way', text: (v) => `${Math.round(v * 360)}°` },
    ],
    glsl: `float fx_scroll_tri(float x) { return abs(mod(x + 1.0, 4.0) - 2.0) - 1.0; }
vec2 fx_scroll(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // Drifting its way with the track (up to a fifth of the height a second), mirrored where it wraps.
  float w = 6.28318530718 * k.y;
  vec2 q = p + a * t * 0.2 * k.x * vec2(cos(w), sin(w));
  return vec2(fx_scroll_tri(q.x), fx_scroll_tri(q.y));
}`,
    report: (a, k) => ({ motion: 0.2 * k[0] * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), saturate
    id: 12,
    name: 'saturate',
    engine: 'hydra',
    kind: 'colour',
    slots: ['picture'],
    knobs: [],
    glsl: `vec3 fx_saturate(vec3 c, vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return c;
  // Its colour drawn out from its grey, up to twice as far.
  float grey = dot(c, vec3(0.2126, 0.7152, 0.0722));
  return clamp(mix(vec3(grey), c, 1.0 + a), 0.0, 1.0);
}`,
    report: () => ({}),
  },
  {
    // After EYESY's Magnify Cloud mode (Critter & Guitari): the idea only, our own shader.
    id: 13,
    name: 'magnify',
    engine: 'kaleidoscope',
    kind: 'shape',
    slots: ['picture'],
    knobs: [
      { label: 'Size', text: pct },
      { label: 'Power', text: (v) => `${(1 / (1 - between(0.3, 0.9, v))).toFixed(1)}×` },
    ],
    glsl: `vec2 fx_magnify(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // A lens of its size (a quarter to nine tenths of the height across) wandering slowly about the middle, magnifying
  // what is under it by its power at its heart.
  vec2 m = 0.45 * vec2(sin(0.13 * t), cos(0.17 * t));
  float r = mix(0.25, 0.9, k.x);
  float d = length(p - m) / r;
  if (d >= 1.0) return p;
  return m + (p - m) * (1.0 - mix(0.3, 0.9, k.y) * a * (1.0 - d * d));
}`,
    report: (a) => ({ shapeChurn: 0.08 * a }),
  },
  {
    // After EYESY's Concentric and Radiating Square modes (Critter & Guitari): the idea only, our own shader.
    id: 14,
    name: 'rings',
    engine: 'eyedo',
    kind: 'shape',
    slots: ['picture', 'loop'],
    knobs: [
      { label: 'Count', text: (v) => `${Math.round(between(6, 30, v))}` },
      { label: 'Speed', text: pct },
    ],
    glsl: `vec2 fx_rings(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // Ripples running out from the middle (6 to 30 to the height, outward at one to eight a second), pushing what is
  // read in and out along the way.
  float r = length(p);
  if (r < 1e-4) return p;
  return p + (p / r) * 0.05 * a * sin(mix(6.0, 30.0, k.x) * r - mix(1.0, 8.0, k.y) * t);
}`,
    report: (a, k) => ({ shapeChurn: 0.2 * k[1] * a }),
  },
  {
    // After EYESY's Interference mode (Critter & Guitari): the idea only, our own shader.
    id: 15,
    name: 'interference',
    engine: 'eyedo',
    kind: 'colour',
    slots: ['picture'],
    knobs: [
      { label: 'Scale', text: (v) => `${Math.round(between(10, 40, v))}` },
      { label: 'Drift', text: pct },
    ],
    glsl: `vec3 fx_interference(vec3 c, vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return c;
  // Two sets of rings (their scale 10 to 40) from two points drifting at its drift, laid over each other: their moiré
  // darkens the picture in bands (never brightens it, so it cannot flash).
  float d = 0.14 * k.y;
  vec2 c1 = 0.6 * vec2(sin(d * t), cos(0.7 * d * t));
  vec2 c2 = -c1;
  float s = mix(10.0, 40.0, k.x);
  float m = 0.5 + 0.5 * cos(s * length(p - c1)) * cos(s * length(p - c2));
  return c * mix(1.0, m, 0.7 * a);
}`,
    report: (a, k) => ({ shapeChurn: 0.08 * k[1] * a }),
  },
  {
    // After EYESY's trigger modes (Critter & Guitari): the idea only, our own shader.
    id: 16,
    name: 'pulse',
    engine: 'eyedo',
    kind: 'shape',
    slots: ['picture'],
    knobs: [
      { label: 'Swell', text: (v) => `${Math.round(between(0.03, 0.25, v) * 100)}%` },
      { label: 'Decay', text: (v) => `${Math.round(between(0.06, 0.4, v) * 1000)} ms` },
    ],
    glsl: `vec2 fx_pulse(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // On each beat the picture swells (by its swell, as strong as the beat) and eases back over its decay.
  vec4 b = fx_beat();
  return p / (1.0 + mix(0.03, 0.25, k.x) * a * b.z * exp(-b.y / mix(0.06, 0.4, k.y)));
}`,
    report: (a, k) => ({ zoomPumping: 3 * between(0.03, 0.25, k[0]) * a }),
  },
  {
    // After EYESY's trigger modes (Critter & Guitari): the idea only, our own shader.
    id: 17,
    name: 'burst',
    engine: 'eyedo',
    kind: 'shape',
    slots: ['picture'],
    knobs: [
      { label: 'Speed', text: pct },
      { label: 'Width', text: pct },
    ],
    glsl: `vec2 fx_burst(vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return p;
  // On each beat a ring of motion (its width) runs out from the middle (one to four heights a second, its speed),
  // pushing what it passes outward, fading over half a second.
  vec4 b = fx_beat();
  float r = length(p);
  if (r < 1e-4) return p;
  float ring = exp(-pow((r - mix(1.0, 4.0, k.x) * b.y) / mix(0.04, 0.2, k.y), 2.0));
  return p - (p / r) * 0.07 * a * b.z * ring * exp(-b.y / 0.5);
}`,
    report: (a) => ({ shapeChurn: 0.12 * a }),
  },
  {
    // After EYESY's trigger modes (Critter & Guitari): the idea only, our own shader.
    id: 18,
    name: 'halo',
    engine: 'eyedo',
    kind: 'colour',
    slots: ['picture'],
    knobs: [
      { label: 'Speed', text: pct },
      { label: 'Width', text: pct },
    ],
    glsl: `vec3 fx_halo(vec3 c, vec2 p, float a, float t, vec2 k) {
  if (a <= 0.0) return c;
  // On each beat a dark ring (its width) runs out from the middle (its speed) and fades: it only ever darkens, so it
  // cannot flash.
  vec4 b = fx_beat();
  float ring = exp(-pow((length(p) - mix(0.8, 3.0, k.x) * b.y) / mix(0.03, 0.15, k.y), 2.0));
  return c * (1.0 - 0.6 * a * b.z * ring * exp(-b.y / 0.7));
}`,
    report: (a) => ({ shapeChurn: 0.05 * a }),
  },
];

const byId = new Map(EFFECTS.map((e) => [e.id, e]));

/** Each engine's sub-effects in the order its setting counts them (1 the first, 0 none; a new one goes at the end, so
 *  a kept setting keeps its effect), and the loop's likewise. */
export const ENGINE_EFFECTS: Record<Engine, readonly Effect[]> = {
  kaleidoscope: [1, 3, 10, 11, 4, 13].map((id) => byId.get(id)!),
  eyedo: [16, 17, 18, 14, 15].map((id) => byId.get(id)!),
  hydra: [9, 7, 8, 5, 6, 12, 2].map((id) => byId.get(id)!),
};
export const LOOP_EFFECTS: readonly Effect[] = [1, 4, 7, 8, 10, 11, 14].map((id) => byId.get(id)!);
/** An engine's sub-effect at `place` (1..), or the loop's: its id (0: none). */
export const engineEffect = (engine: Engine, place: number) => ENGINE_EFFECTS[engine][Math.round(place) - 1]?.id ?? 0;
export const loopEffect = (place: number) => LOOP_EFFECTS[Math.round(place) - 1]?.id ?? 0;
/** An effect by its id (none for 0 or one not known: a setting from a later version shows nothing, rather than fails). */
export const effectOf = (id: number) => byId.get(id) ?? null;

/** The most effects in the picture's chain. */
export const MOST_PICTURE = 3;

/** A chain's ids as kept (0: none), its effects in order, those not known or not for the slot left out. */
export function chainOf(ids: readonly number[], slot: 'picture' | 'loop'): Effect[] {
  return ids.map(effectOf).filter((e): e is Effect => !!e && e.slots.includes(slot));
}

const HALF: Knobs = [0.5, 0.5];

/** What a chain adds to the meters a second at its amounts and knobs (each id's by place). */
export function reportOf(ids: readonly number[], amounts: readonly number[], slot: 'picture' | 'loop', knobs: readonly Knobs[] = []): Report {
  const out: Report = {};
  ids.forEach((id, k) => {
    const e = effectOf(id);
    if (!e || !e.slots.includes(slot) || !(amounts[k] > 0)) return;
    for (const [m, v] of Object.entries(e.report(amounts[k], knobs[k] ?? HALF)) as [keyof Report, number][]) out[m] = (out[m] ?? 0) + v;
  });
  return out;
}

const COMMON = `vec3 fx_prev(sampler2D s, vec2 q) { return textureLod(s, vec2(q.x / uAspect, q.y) * 0.5 + 0.5, 0.0).rgb; }
// The beat now: where in it, how long since it fell (s), its strength, where in its bar (4/4).
vec4 fx_beat() { return uFxBeat; }
// The fluid's flow where p is (p's units a second); none where nothing flows.
vec2 fx_flow(vec2 p) {
  if (uFlowScale.x == 0.0) return vec2(0.0);
  vec2 v = textureLod(uFlow, vec2(p.x / uAspect, p.y) * 0.5 + 0.5, 0.0).xy * uFlowScale;
  return v * 2.0 * vec2(uAspect, 1.0);
}`;

/** The picture slot's fragment shader for a chain (its ids by place, as `uFxA`'s and `uFxK`'s), or null where nothing
 *  is in it: the picture (`uPicture`) read where its shapes say, then its colours and echoes (`uPrev`: the last
 *  finished one). */
export function pictureShader(ids: readonly number[]): string | null {
  const placed = ids.map((id, k) => ({ e: effectOf(id), k })).filter((x): x is { e: Effect; k: number } => !!x.e && x.e.slots.includes('picture'));
  if (!placed.length) return null;
  const once = [...new Map(placed.map(({ e }) => [e.id, e.glsl])).values()].join('\n');
  const call = ({ e, k }: { e: Effect; k: number }) =>
    e.kind === 'shape'
      ? `  p = fx_${e.name}(p, uFxA[${k}], uFxT, uFxK[${k}]);`
      : e.kind === 'colour'
        ? `  c = fx_${e.name}(c, p, uFxA[${k}], uFxT, uFxK[${k}]);`
        : `  c = fx_${e.name}(c, p, uFxA[${k}], uFxT, uFxK[${k}], uPrev);`;
  return `#version 300 es
precision highp float;
uniform sampler2D uPicture;
uniform sampler2D uPrev;
uniform vec2 uView;
uniform float uAspect;
uniform float uFxA[${MOST_PICTURE}];
uniform vec2 uFxK[${MOST_PICTURE}];
uniform float uFxT;
uniform vec4 uFxBeat;
uniform sampler2D uFlow;
uniform vec2 uFlowScale;
out vec4 o;
${COMMON}
${once}
void main() {
  vec2 p = (gl_FragCoord.xy / uView - 0.5) * 2.0 * vec2(uAspect, 1.0);
${placed.filter(({ e }) => e.kind === 'shape').map(call).join('\n')}
  vec3 c = textureLod(uPicture, vec2(p.x / uAspect, p.y) * 0.5 + 0.5, 0.0).rgb;
${placed.filter(({ e }) => e.kind !== 'shape').map(call).join('\n')}
  o = vec4(clamp(c, 0.0, 1.0), 1.0);
}`;
}

/** `fx_flow` where nothing flows (the loop's carry pass): none. The fluid's light pass gives its own (fluid.ts). */
export const NO_FLOW = `vec2 fx_flow(vec2 p) { return vec2(0.0); }`;

/** The loop slot's GLSL for one effect: `vec2 fx_loop(vec2 uv, float a, float t, float aspect, vec2 k)` on a buffer's
 *  coordinates (0..1), for the passes that read the kept light back; none (the identity) where it is not a shape. */
export function loopFunction(id: number, flow = NO_FLOW): string {
  const e = effectOf(id);
  if (!e || !e.slots.includes('loop') || e.kind !== 'shape')
    return `vec2 fx_loop(vec2 uv, float a, float t, float aspect, vec2 k) { return uv; }`;
  return `${flow}
${e.glsl}
vec2 fx_loop(vec2 uv, float a, float t, float aspect, vec2 k) {
  vec2 p = (uv - 0.5) * 2.0 * vec2(aspect, 1.0);
  p = fx_${e.name}(p, a, t, k);
  return vec2(p.x / aspect, p.y) * 0.5 + 0.5;
}`;
}

/** The look's effects as a setting holds them (settings.svelte.ts). */
export type EffectSettings = Pick<
  VisualSettings,
  | 'fx1'
  | 'fx2'
  | 'fx3'
  | 'fx1Amount'
  | 'fx2Amount'
  | 'fx3Amount'
  | 'fxLoop'
  | 'fxLoopAmount'
  | 'fx1K1'
  | 'fx1K2'
  | 'fx2K1'
  | 'fx2K2'
  | 'fx3K1'
  | 'fx3K2'
  | 'fxLoopK1'
  | 'fxLoopK2'
>;

/** Nothing held back. */
const FREE: Levers = { swing: 1, pace: 1, evolution: 1, hue: 1, size: 1, gap: 1 };

/** How far the governor lets an effect through (governor.ts): the flashes' swing always (any effect can flash), and the
 *  lever of every meter it counts toward (its report's), as each holds the flame back: motion the pace, shape churn the
 *  evolving (stretched: 1/evolution), colour churn the hues, zoom pumping the size. Its reports are in step with its
 *  amount, so each lever lowers just what its meter counts. */
export function heldBack(id: number, amount: number, k: Knobs, levers: Levers): number {
  const e = effectOf(id);
  if (!e || !(amount > 0)) return 1;
  const r = e.report(amount, k);
  let f = levers.swing;
  if (r.motion) f *= levers.pace;
  if (r.shapeChurn) f /= Math.max(1, levers.evolution);
  if (r.colourChurn) f *= levers.hue;
  if (r.zoomPumping) f *= levers.size;
  return Math.max(0, Math.min(1, f));
}

/** The look's effects for the renderer (render.ts `Shown.effects`): each amount times what the Effects setting's routes
 *  let through (`level`) and what the governor lets through (`levers`); each one's knobs; `t` the track's own time
 *  (s); the beat. */
export const effectsNow = (how: EffectSettings, level: number, t: number, beat: Beat | null = null, levers: Levers = FREE) => {
  // Each engine's sub-effect (its setting the place in the engine's list), in the engines' order.
  const ids = [how.fx1, how.fx2, how.fx3].map((place, k) => engineEffect(ENGINES[k].key, place));
  const knobs = [
    [how.fx1K1, how.fx1K2],
    [how.fx2K1, how.fx2K2],
    [how.fx3K1, how.fx3K2],
  ] as Knobs[];
  const loop = loopEffect(how.fxLoop);
  const loopKnobs = [how.fxLoopK1, how.fxLoopK2] as Knobs;
  const set = [how.fx1Amount, how.fx2Amount, how.fx3Amount].map((a) => a * level);
  return {
    ids,
    amounts: set.map((a, k) => a * heldBack(ids[k], a, knobs[k], levers)),
    knobs,
    loop,
    loopAmount: how.fxLoopAmount * level * heldBack(loop, how.fxLoopAmount * level, loopKnobs, levers),
    loopKnobs,
    t,
    beat: beatOf(beat),
  };
};

/** The beat as the visuals know it (music.svelte.ts): its number, how long since it fell, how long it lasts (s), its
 *  strength. None (paused, reduced motion, a track without beats): long ago, so a beat's effect rests. */
export type Beat = { index: number; since: number; length: number; strength: number };
export const beatOf = (b: Beat | null): [number, number, number, number] =>
  b && b.length > 0
    ? [Math.min(1, Math.max(0, b.since / b.length)), Math.max(0, b.since), Math.min(1, Math.max(0, b.strength)), (((b.index % 4) + 4) % 4 + Math.min(1, b.since / b.length)) / 4]
    : [0, 1000, 0, 0];

/** What the look's effects add to the meters a second, the picture's and the loop's together: the same for the visuals'
 *  meters and the precheck's, so the governor and the dice hold an effect alike. */
export function settingsReport(how: EffectSettings, level: number, levers: Levers = FREE): Report {
  const fx = effectsNow(how, level, 0, null, levers);
  const out: Report = { ...reportOf(fx.ids, fx.amounts, 'picture', fx.knobs) };
  for (const [k, v] of Object.entries(reportOf([fx.loop], [fx.loopAmount], 'loop', [fx.loopKnobs])) as [keyof Report, number][]) out[k] = (out[k] ?? 0) + v;
  return out;
}
