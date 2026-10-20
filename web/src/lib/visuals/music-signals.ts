// Music per signal (the owner, 2026-10-20: "to cut the cables"): instead of one Music hub with every signal as an output,
// small music modules, one signal each, used as often as wanted and placed beside what they feed. The type's shape is
// a5's to declare: per-signal types (`music-beat`, `music-hits` …) or one `music-signal` type choosing its signal by a
// `signal` step knob (an index into the signals below); `signalOf` answers for either, so the graph works with both.
// Until a5's type lands, `signalTypes` stands in (one type a signal, its one output Music's). Pure.

import type { Module, ModuleType } from './patch';
import { MUSIC, type CableV2, type PatchV2, type Port, type Ports } from './ports';

/** Every signal Music gives, in Music's order (its outputs). */
export const MUSIC_SIGNALS: readonly Port[] = MUSIC.ports.out;

/** The one type that chooses its signal, where a5 declares it so. */
export const MUSIC_SIGNAL_TYPE = 'music-signal';
const PREFIX = 'music-';

/** The signal a module gives, where it is a music module of one signal (not the hub): its port. */
export function signalOf(m: Pick<Module, 'type' | 'knobs'>): Port | null {
  if (m.type === MUSIC_SIGNAL_TYPE) return MUSIC_SIGNALS[Math.round(m.knobs.signal ?? 0)] ?? null;
  if (!m.type.startsWith(PREFIX)) return null;
  const id = m.type.slice(PREFIX.length);
  return MUSIC_SIGNALS.find((s) => s.id === id) ?? null;
}

/** A new module giving `signal`, as `types` declares music modules (one type choosing, else a type a signal). */
export function musicModuleFor(signal: string, types: Record<string, ModuleType>): { type: string; knobs: Record<string, number> } {
  if (types[MUSIC_SIGNAL_TYPE]) return { type: MUSIC_SIGNAL_TYPE, knobs: { signal: Math.max(0, MUSIC_SIGNALS.findIndex((s) => s.id === signal)) } };
  return { type: `${PREFIX}${signal}`, knobs: {} };
}

/** The stand-in types (until a5's): a modulator of one output each, Music's port as it is. */
export function signalTypes(): Record<string, ModuleType & { ports: Ports }> {
  return Object.fromEntries(
    MUSIC_SIGNALS.map((s): [string, ModuleType & { ports: Ports }] => [`${PREFIX}${s.id}`, { type: `${PREFIX}${s.id}`, label: s.label, place: 'modulator', knobs: [], ports: { in: [], out: [{ ...s }] } }]),
  );
}

/** The hub split (what toV2 will do, for the lab): each cable from Music now comes from a module of its one signal,
 *  one for each signal and module it feeds (so each sits beside what it feeds); the hub goes where no cable is left
 *  from it. Ids `<signal>-<target>`. */
export function splitMusic(p: PatchV2): PatchV2 {
  const hub = p.modules.find((m) => m.type === 'music');
  if (!hub) return p;
  const made = new Map<string, Module>();
  const cables: CableV2[] = p.cables.map((c) => {
    if (c.from.module !== hub.id) return c;
    const id = `${c.from.port.replace(/[^\w-]/g, '_')}-${c.to.module}`;
    if (!made.has(id)) made.set(id, { id, type: `${PREFIX}${c.from.port}`, knobs: {} });
    return { ...c, from: { ...c.from, module: id } };
  });
  const left = cables.some((c) => c.from.module === hub.id || c.to.module === hub.id || c.depth?.module === hub.id);
  return { ...p, modules: [...p.modules.filter((m) => left || m !== hub), ...made.values()], cables };
}
