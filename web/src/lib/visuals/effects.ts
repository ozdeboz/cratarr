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
// A chain is a few effects' ids with their amounts; it is compiled into one shader a slot, rebuilt only when its ids
// change: the amounts are uniforms. Each effect says what it adds to the meters (meters.ts), so the governor holds it
// back as it holds the flame back. Pure: no GL here.

import type { VisualSettings } from './settings.svelte';

/** The meters an effect adds to, each a second at its amount (the meters' own units). */
export type Report = Partial<Record<'motion' | 'shapeChurn' | 'colourChurn' | 'zoomPumping', number>>;

export type Effect = {
  /** Stable, kept in settings: never reused. */
  id: number;
  /** The panel's label. */
  name: string;
  /** shape: \`vec2 fx_<name>(vec2 p, float a, float t)\`, where to read instead; colour: \`vec3 fx_<name>(vec3 c, vec2 p,
   *  float a, float t)\`; echo: \`vec3 fx_<name>(vec3 c, vec2 p, float a, float t, sampler2D prev)\` (sample it with
   *  \`fx_prev(prev, q)\`). p: centred, aspect-true (y −1..1, x ±aspect); a: its amount (0: none, it must change
   *  nothing); t: the track's own time (seconds). */
  kind: 'shape' | 'colour' | 'echo';
  /** Where it may go: the loop takes shapes only. */
  slots: readonly ('picture' | 'loop')[];
  /** One GLSL ES 3.00 function \`fx_<name>\`, and helpers named \`fx_<name>_…\`. */
  glsl: string;
  report: (amount: number) => Report;
};

export const EFFECTS: readonly Effect[] = [
  {
    // ported from hydra-synth (AGPL-3.0), kaleid
    id: 1,
    name: 'kaleid',
    kind: 'shape',
    slots: ['picture', 'loop'],
    glsl: `vec2 fx_kaleid(vec2 p, float a, float t) {
  if (a <= 0.0) return p;
  // As many sides as its amount asks (2 to 12), turning slowly.
  float n = floor(2.0 + a * 10.0);
  float r = length(p);
  float th = atan(p.y, p.x) + t * 0.05 * a;
  float side = 6.28318530718 / n;
  th = mod(th, side);
  th = min(th, side - th);
  return r * vec2(cos(th), sin(th));
}`,
    report: (a) => ({ motion: 0.05 * a, shapeChurn: 0.1 * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), posterize
    id: 2,
    name: 'posterize',
    kind: 'colour',
    slots: ['picture'],
    glsl: `vec3 fx_posterize(vec3 c, vec2 p, float a, float t) {
  if (a <= 0.0) return c;
  // From many steps (none to see) down to three a channel.
  float steps = mix(64.0, 3.0, a);
  return mix(c, floor(c * steps + 0.5) / steps, min(1.0, a * 2.0));
}`,
    report: () => ({}),
  },
  {
    // ported from hydra-synth (AGPL-3.0), repeat
    id: 3,
    name: 'repeat',
    kind: 'shape',
    slots: ['picture'],
    glsl: `float fx_repeat_tri(float x) { return abs(mod(x + 1.0, 4.0) - 2.0) - 1.0; }
vec2 fx_repeat(vec2 p, float a, float t) {
  if (a <= 0.0) return p;
  // From one picture to four across, mirrored at each seam so none shows; eased in from none.
  float n = 1.0 + 3.0 * a;
  vec2 tiled = vec2(fx_repeat_tri(p.x * n), fx_repeat_tri(p.y * n));
  return mix(p, tiled, smoothstep(0.0, 0.15, a));
}`,
    report: (a) => ({ shapeChurn: 0.05 * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), pixelate
    id: 4,
    name: 'pixelate',
    kind: 'shape',
    slots: ['picture', 'loop'],
    glsl: `vec2 fx_pixelate(vec2 p, float a, float t) {
  if (a <= 0.0) return p;
  // From fine (two hundred cells across the height) to coarse (eight).
  float cells = 0.5 * mix(200.0, 8.0, a * a);
  return (floor(p * cells) + 0.5) / cells;
}`,
    report: () => ({}),
  },
  {
    // ported from hydra-synth (AGPL-3.0), colorama
    id: 5,
    name: 'colorama',
    kind: 'colour',
    slots: ['picture'],
    glsl: `vec3 fx_colorama_hsv(vec3 c) {
  vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
  vec4 q = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
  vec4 r = mix(vec4(q.xyw, c.r), vec4(c.r, q.yzx), step(q.x, c.r));
  float d = q.x - min(q.w, q.y);
  return vec3(abs(r.z + (r.w - r.y) / (6.0 * d + 1e-10)), d / (r.x + 1e-10), r.x);
}
vec3 fx_colorama_rgb(vec3 c) {
  vec3 p = abs(fract(c.xxx + vec3(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
  return c.z * mix(vec3(1.0), clamp(p - 1.0, 0.0, 1.0), c.y);
}
vec3 fx_colorama(vec3 c, vec2 p, float a, float t) {
  if (a <= 0.0) return c;
  // Each colour's hue moved by its own brightness and slowly round with the track (a twentieth of a turn a second at
  // all of it): the colours shift in bands.
  vec3 h = fx_colorama_hsv(c);
  h.x = fract(h.x + a * (0.6 * h.z + 0.05 * t));
  return fx_colorama_rgb(h);
}`,
    report: (a) => ({ colourChurn: 0.05 * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), hue
    id: 6,
    name: 'hue',
    kind: 'colour',
    slots: ['picture'],
    glsl: `vec3 fx_hue_hsv(vec3 c) {
  vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
  vec4 q = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
  vec4 r = mix(vec4(q.xyw, c.r), vec4(c.r, q.yzx), step(q.x, c.r));
  float d = q.x - min(q.w, q.y);
  return vec3(abs(r.z + (r.w - r.y) / (6.0 * d + 1e-10)), d / (r.x + 1e-10), r.x);
}
vec3 fx_hue_rgb(vec3 c) {
  vec3 p = abs(fract(c.xxx + vec3(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
  return c.z * mix(vec3(1.0), clamp(p - 1.0, 0.0, 1.0), c.y);
}
vec3 fx_hue(vec3 c, vec2 p, float a, float t) {
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
    kind: 'shape',
    slots: ['picture', 'loop'],
    glsl: `float fx_ripple_hash(vec2 q) { return fract(sin(dot(q, vec2(127.1, 311.7))) * 43758.5453); }
float fx_ripple_noise(vec2 q) {
  vec2 i = floor(q);
  vec2 f = fract(q);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(fx_ripple_hash(i), fx_ripple_hash(i + vec2(1.0, 0.0)), u.x), mix(fx_ripple_hash(i + vec2(0.0, 1.0)), fx_ripple_hash(i + vec2(1.0, 1.0)), u.x), u.y);
}
vec2 fx_ripple(vec2 p, float a, float t) {
  if (a <= 0.0) return p;
  // Moved by a slow noise, as far as a tenth of the height at all of it.
  vec2 q = p * 2.0 + vec2(0.0, 0.15 * t);
  vec2 n = vec2(fx_ripple_noise(q), fx_ripple_noise(q + vec2(5.2, 1.3))) - 0.5;
  return p + 0.2 * a * n;
}`,
    report: (a) => ({ shapeChurn: 0.15 * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), modulate (by the visuals' own flow)
    id: 8,
    name: 'stream',
    kind: 'shape',
    slots: ['picture', 'loop'],
    glsl: `vec2 fx_stream(vec2 p, float a, float t) {
  if (a <= 0.0) return p;
  // Read from upstream: the picture drawn along the fluid's flow (nothing where it does not flow), up to a fifth of a
  // second of it.
  return p - 0.2 * a * fx_flow(p);
}`,
    report: (a) => ({ shapeChurn: 0.1 * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), src(o0) fed back, scaled and turned
    id: 9,
    name: 'echo',
    kind: 'echo',
    slots: ['picture'],
    glsl: `vec3 fx_echo(vec3 c, vec2 p, float a, float t, sampler2D prev) {
  if (a <= 0.0) return c;
  // The last picture, a little larger and turned, kept where it is brighter: trails that stream out and round.
  float s = 1.0 + 0.03 * a;
  float r = 0.006 * a;
  vec2 q = mat2(cos(r), -sin(r), sin(r), cos(r)) * p / s;
  return max(c, fx_prev(prev, q) * (0.75 + 0.2 * a));
}`,
    report: (a) => ({ motion: 0.35 * a, zoomPumping: 0 }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), rotate
    id: 10,
    name: 'rotate',
    kind: 'shape',
    slots: ['picture', 'loop'],
    glsl: `vec2 fx_rotate(vec2 p, float a, float t) {
  if (a <= 0.0) return p;
  // The whole turning slowly with the track (a twentieth of a turn a second at all of it).
  float r = 0.314159 * a * t;
  return mat2(cos(r), -sin(r), sin(r), cos(r)) * p;
}`,
    report: (a) => ({ motion: 0.314159 * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), scroll
    id: 11,
    name: 'scroll',
    kind: 'shape',
    slots: ['picture', 'loop'],
    glsl: `float fx_scroll_tri(float x) { return abs(mod(x + 1.0, 4.0) - 2.0) - 1.0; }
vec2 fx_scroll(vec2 p, float a, float t) {
  if (a <= 0.0) return p;
  // Drifting sideways and up with the track, mirrored where it wraps (a tenth of the height a second at all of it).
  vec2 q = p + a * t * vec2(0.1, 0.04);
  return vec2(fx_scroll_tri(q.x), fx_scroll_tri(q.y));
}`,
    report: (a) => ({ motion: 0.1 * a }),
  },
  {
    // ported from hydra-synth (AGPL-3.0), saturate
    id: 12,
    name: 'saturate',
    kind: 'colour',
    slots: ['picture'],
    glsl: `vec3 fx_saturate(vec3 c, vec2 p, float a, float t) {
  if (a <= 0.0) return c;
  // Its colour drawn out from its grey, up to twice as far.
  float grey = dot(c, vec3(0.2126, 0.7152, 0.0722));
  return clamp(mix(vec3(grey), c, 1.0 + a), 0.0, 1.0);
}`,
    report: () => ({}),
  },
];

const byId = new Map(EFFECTS.map((e) => [e.id, e]));
/** An effect by its id (none for 0 or one not known: a setting from a later version shows nothing, rather than fails). */
export const effectOf = (id: number) => byId.get(id) ?? null;

/** The most effects in the picture's chain. */
export const MOST_PICTURE = 3;

/** A chain's ids as kept (0: none), its effects in order, those not known or not for the slot left out. */
export function chainOf(ids: readonly number[], slot: 'picture' | 'loop'): Effect[] {
  return ids.map(effectOf).filter((e): e is Effect => !!e && e.slots.includes(slot));
}

/** What a chain adds to the meters a second at its amounts (each id's amount by place). */
export function reportOf(ids: readonly number[], amounts: readonly number[], slot: 'picture' | 'loop'): Report {
  const out: Report = {};
  ids.forEach((id, k) => {
    const e = effectOf(id);
    if (!e || !e.slots.includes(slot) || !(amounts[k] > 0)) return;
    for (const [m, v] of Object.entries(e.report(amounts[k])) as [keyof Report, number][]) out[m] = (out[m] ?? 0) + v;
  });
  return out;
}

const COMMON = `vec3 fx_prev(sampler2D s, vec2 q) { return textureLod(s, vec2(q.x / uAspect, q.y) * 0.5 + 0.5, 0.0).rgb; }
// The fluid's flow where p is (p's units a second); none where nothing flows.
vec2 fx_flow(vec2 p) {
  if (uFlowScale.x == 0.0) return vec2(0.0);
  vec2 v = textureLod(uFlow, vec2(p.x / uAspect, p.y) * 0.5 + 0.5, 0.0).xy * uFlowScale;
  return v * 2.0 * vec2(uAspect, 1.0);
}`;

/** The picture slot's fragment shader for a chain (its ids by place, as \`uFxA\`'s), or null where nothing is in it:
 *  the picture (\`uPicture\`) read where its shapes say, then its colours and echoes (\`uPrev\`: the last finished one). */
export function pictureShader(ids: readonly number[]): string | null {
  const placed = ids.map((id, k) => ({ e: effectOf(id), k })).filter((x): x is { e: Effect; k: number } => !!x.e && x.e.slots.includes('picture'));
  if (!placed.length) return null;
  const once = [...new Map(placed.map(({ e }) => [e.id, e.glsl])).values()].join('\n');
  const call = ({ e, k }: { e: Effect; k: number }) =>
    e.kind === 'shape'
      ? `  p = fx_${e.name}(p, uFxA[${k}], uFxT);`
      : e.kind === 'colour'
        ? `  c = fx_${e.name}(c, p, uFxA[${k}], uFxT);`
        : `  c = fx_${e.name}(c, p, uFxA[${k}], uFxT, uPrev);`;
  return `#version 300 es
precision highp float;
uniform sampler2D uPicture;
uniform sampler2D uPrev;
uniform vec2 uView;
uniform float uAspect;
uniform float uFxA[${MOST_PICTURE}];
uniform float uFxT;
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

/** The loop slot's GLSL for one effect: \`vec2 fx_loop(vec2 uv, float a, float t, float aspect)\` on a buffer's
 *  coordinates (0..1), for the passes that read the kept light back; none (the identity) where it is not a shape. */
export function loopFunction(id: number): string {
  const e = effectOf(id);
  if (!e || !e.slots.includes('loop') || e.kind !== 'shape')
    return `vec2 fx_loop(vec2 uv, float a, float t, float aspect) { return uv; }`;
  return `${e.glsl}
vec2 fx_loop(vec2 uv, float a, float t, float aspect) {
  vec2 p = (uv - 0.5) * 2.0 * vec2(aspect, 1.0);
  p = fx_${e.name}(p, a, t);
  return vec2(p.x / aspect, p.y) * 0.5 + 0.5;
}`;
}

/** The look's effects as a setting holds them (settings.svelte.ts). */
export type EffectSettings = Pick<VisualSettings, 'fx1' | 'fx2' | 'fx3' | 'fx1Amount' | 'fx2Amount' | 'fx3Amount' | 'fxLoop' | 'fxLoopAmount'>;

/** The look's effects for the renderer (render.ts `Shown.effects`): each amount times what the Effects setting's routes
 *  let through (`level`); `t` the track's own time (s). */
export const effectsNow = (how: EffectSettings, level: number, t: number) => ({
  ids: [how.fx1, how.fx2, how.fx3].map(Math.round),
  amounts: [how.fx1Amount, how.fx2Amount, how.fx3Amount].map((a) => a * level),
  loop: Math.round(how.fxLoop),
  loopAmount: how.fxLoopAmount * level,
  t,
});

/** What the look's effects add to the meters a second, the picture's and the loop's together: the same for the visuals'
 *  meters and the precheck's, so the governor and the dice hold an effect alike. */
export function settingsReport(how: EffectSettings, level: number): Report {
  const fx = effectsNow(how, level, 0);
  const out: Report = { ...reportOf(fx.ids, fx.amounts, 'picture') };
  for (const [k, v] of Object.entries(reportOf([fx.loop], [fx.loopAmount], 'loop')) as [keyof Report, number][]) out[k] = (out[k] ?? 0) + v;
  return out;
}
