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
