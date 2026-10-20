// One module per Music output (0057, the owner: `music-<signal>`, ports.ts MUSIC_SIGNAL_MODULES): the engine reads a
// patch with them as one with the Music hub, each module's `out` Music's port of its signal read by the module's own
// `reference` knob (a row's reading), so everything that reads Music (the readings, the bands' channels, the ADSRs'
// gates, the fluid's pushes) reads them alike.

import { musicSignalOf, type PatchV2 } from '../ports';

const views = new WeakMap<PatchV2, PatchV2>();

/** `p` with its Music signal modules read as Music's ports (the same object where it has none). Worked out once a
 *  patch object; a reading changed is a structure change (live-patch.ts). */
export function musicView(p: PatchV2): PatchV2 {
  const kept = views.get(p);
  if (kept) return kept;
  const signals = p.modules.filter((m) => musicSignalOf(m.type));
  if (!signals.length) {
    views.set(p, p);
    return p;
  }
  const hub = p.modules.find((m) => m.type === 'music');
  const music = hub ? { ...hub, knobs: { ...hub.knobs } } : { id: 'music', type: 'music', knobs: {} as Record<string, number> };
  const signalOf = new Map(signals.map((m) => [m.id, musicSignalOf(m.type)!]));
  for (const m of signals) {
    const signal = signalOf.get(m.id)!;
    if (typeof m.knobs.reference === 'number') music.knobs[`reference.${signal}`] = m.knobs.reference;
    for (const [k, v] of Object.entries(m.knobs)) if (k.startsWith('hold.')) music.knobs[k] = v;
  }
  const view: PatchV2 = {
    ...p,
    modules: [music, ...p.modules.filter((m) => m !== hub && !signalOf.has(m.id))],
    cables: p.cables.map((c) => (signalOf.has(c.from.module) ? { ...c, from: { ...c.from, module: music.id, port: signalOf.get(c.from.module)! } } : c)),
  };
  views.set(p, view);
  return view;
}
