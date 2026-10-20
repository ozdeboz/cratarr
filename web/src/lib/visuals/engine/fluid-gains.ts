// What each of the music's pushes is worth to the fluid (decision 0057, the switch; stirring.ts asks it as `gain`): the
// cables from that signal (that band's, or the whole track's) into the Cratarr device's Fluid's Stir, Flow or Swirl,
// each its curve's height at the push's strength, added up. What the routes' matrix (heightOn) and the extras' cables
// (chain-cables.ts cableGain) gave between them, from the patch's own cables. The loudness comes from the Loudness's
// level, the Track's pulse from the ADSR on a new track, the hits through their ADSRs, a slow signal through its
// Envelope Follower (v2.ts), as toV2 wires them.

import { heightOf, shapeOf, type Shape } from '../evaluate';
import type { PatchV2 } from '../ports';
import { BAND_SIGNALS } from '../routes';
import { followsOf } from './follow';
import { musicView } from './music-signals';

export type FluidAspect = 'stir' | 'flow' | 'swirl';
export type FluidGain = (signal: string, band: number | null, aspect: FluidAspect, x: number) => number;

const BANDS = new Set<string>(BAND_SIGNALS);
const gains = new WeakMap<PatchV2, Map<string, Shape[]>>();

/** The patch's fluid gains (a flattened patch's; worked out once a patch object). */
export function fluidGains(p: PatchV2): FluidGain {
  let index = gains.get(p);
  if (!index) gains.set(p, (index = indexOf(musicView(p))));
  const by = index;
  return (signal, band, aspect, x) => {
    let sum = 0;
    for (const sh of by.get(`${aspect}|${signal}|${band ?? '-'}`) ?? []) sum += heightOf(sh, x);
    return sum;
  };
}

function indexOf(p: PatchV2): Map<string, Shape[]> {
  const out = new Map<string, Shape[]>();
  const typeOf = new Map(p.modules.map((m) => [m.id, m.type]));
  const fluid = p.modules.find((m) => m.type === 'fluid');
  if (!fluid) return out;
  // Each ADSR's gate: the Music signal it strikes on (a new track's: the track's pulse). Each Envelope Follower's input:
  // the Music signal it follows (and its band); the Loudness's, uncabled, the loudness.
  const gate = new Map<string, string>();
  for (const c of p.cables) if (typeOf.get(c.to.module) === 'adsr' && c.to.port === 'gate') gate.set(c.to.module, c.from.port === 'newtrack' ? 'track' : c.from.port);
  const follows = new Map<string, { signal: string; channel?: number }>();
  for (const c of p.cables) if (typeOf.get(c.to.module) === 'envelope' && c.to.port === 'in' && typeOf.get(c.from.module) === 'music') follows.set(c.to.module, { signal: c.from.port, channel: c.from.channel });
  const add = (aspect: string, signal: string, band: number | null, sh: Shape) => {
    const key = `${aspect}|${signal}|${band ?? '-'}`;
    (out.get(key) ?? out.set(key, []).get(key)!).push(sh);
  };
  // A band's cables into a column that is the whole track's come through a Merge (its mean: cablesAdd in v2.ts); each
  // is that band's own push, as the stirring asks it band by band.
  const into = new Map<string, string>();
  for (const c of p.cables) if (c.to.module === fluid.id && ['stir', 'flow', 'swirl'].includes(c.to.port) && typeOf.get(c.from.module) === 'merge') into.set(c.from.module, c.to.port);
  const pushes = p.cables.flatMap((c) => {
    if (c.to.module === fluid.id && ['stir', 'flow', 'swirl'].includes(c.to.port)) return typeOf.get(c.from.module) === 'merge' ? [] : [{ c, aspect: c.to.port, channel: c.from.channel }];
    const aspect = into.get(c.to.module);
    return aspect && c.to.port === 'in' ? [{ c, aspect, channel: c.to.channel ?? c.from.channel }] : [];
  });
  // Through the Loudness Follow's VCAs to the signal behind each (its curve, a curved cable's, on the cable in).
  const throughVca = followsOf(p);
  for (const push of pushes) {
    const via = throughVca.get(push.c.from.module);
    const c = via ? { ...push.c, from: via.from, value: typeof push.c.value === 'number' && push.c.value === 1 ? via.value : push.c.value } : push.c;
    const { aspect, channel: given } = push;
    const type = typeOf.get(c.from.module) ?? c.from.module;
    const sh = shapeOf(c.value);
    const followed = type === 'envelope' ? follows.get(c.from.module) : undefined;
    const channel = given ?? followed?.channel;
    const signal = type === 'envelope' ? (followed?.signal ?? (c.from.port === 'level' ? 'loudness' : undefined)) : type === 'adsr' ? gate.get(c.from.module) : type === 'music' ? c.from.port : undefined;
    if (!signal) continue;
    // A band's signal with no channel of its own: every band's (a polyphonic cable).
    if (channel === undefined && BANDS.has(signal)) for (let b = 0; b < 6; b++) add(aspect, signal, b, sh);
    else add(aspect, signal, channel ?? null, sh);
  }
  return out;
}
