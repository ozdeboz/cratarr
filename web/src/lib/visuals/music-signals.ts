// Music per signal (the owner, 2026-10-20: "to cut the cables"): instead of one Music hub with every signal as an output,
// small music modules, one signal each (ports.ts MUSIC_SIGNAL_MODULES: `music-<signal>`, its one output `out`), used
// as often as wanted and placed beside what they feed. The graph's side: which signal a module gives, a new one, and
// (for the lab, what toV2 does) the hub split into them. Pure.

import type { Module } from './patch';
import { MUSIC, musicSignalOf, type CableV2, type PatchV2, type Port } from './ports';

/** Every signal Music gives, in Music's order (its outputs). */
export const MUSIC_SIGNALS: readonly Port[] = MUSIC.ports.out;
/** A music module's one output. */
export const SIGNAL_OUT = 'out';

/** The signal a module gives, where it is a music module of one signal (not the hub). */
export function signalOf(m: Pick<Module, 'type'>): Port | null {
  const id = musicSignalOf(m.type);
  return id === null ? null : (MUSIC_SIGNALS.find((s) => s.id === id) ?? null);
}

/** A new module giving `signal`. */
export const musicModuleFor = (signal: string): { type: string; knobs: Record<string, number> } => ({ type: `music-${signal}`, knobs: {} });

/** The hub split (toV2's, for the lab): each cable from Music now comes from a module of its one signal, one for each
 *  signal and module it feeds (so each sits beside what it feeds); the hub goes where nothing is left on it. Ids
 *  `<signal>-<target>`. */
export function splitMusic(p: PatchV2): PatchV2 {
  const hub = p.modules.find((m) => m.type === 'music');
  if (!hub) return p;
  const made = new Map<string, Module>();
  const cables: CableV2[] = p.cables.map((c) => {
    if (c.from.module !== hub.id) return c;
    const id = `${c.from.port.replace(/[^\w-]/g, '_')}-${c.to.module}`;
    if (!made.has(id)) made.set(id, { id, type: `music-${c.from.port}`, knobs: {} });
    return { ...c, from: { ...c.from, module: id, port: SIGNAL_OUT } };
  });
  const left = cables.some((c) => c.from.module === hub.id || c.to.module === hub.id || c.depth?.module === hub.id);
  return { ...p, modules: [...p.modules.filter((m) => left || m !== hub), ...made.values()], cables };
}
