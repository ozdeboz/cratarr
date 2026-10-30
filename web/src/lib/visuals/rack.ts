// The five categories (decision 0057 §3b, the owner agreed): what a module does with signals, as modular synths and
// video synthesizers group them. Sources make a picture, Effects take one and give one, Mixers make one of several,
// Modulators make the signals that move knobs, In / Out is where signals enter and leave. The graph's lanes, the module
// picker and Randomize's scopes all follow them. Pure.

import { portsOfModule, type PatchV2, type Ports } from './ports';
import type { Module, ModuleType, Place } from './patch';

export const CATEGORIES = ['sources', 'effects', 'mixers', 'modulators', 'io'] as const;
export type Category = (typeof CATEGORIES)[number];
export type ByCategory = Record<Category, Module[]>;

export const CATEGORY_LABEL: Record<Category, string> = { sources: 'Sources', effects: 'Effects', mixers: 'Mixers', modulators: 'Modulators', io: 'In / Out' };

/** The types whose category their place does not tell. */
const DECLARED: Record<string, Category> = {
  mix: 'mixers',
  feedback: 'mixers',
  mask: 'mixers',
  ducker: 'modulators',
  music: 'io',
  filterbank: 'io',
  meter: 'io',
  screen: 'io',
  frame: 'io',
};

const BY_PLACE: Record<Place, Category> = { source: 'sources', module: 'effects', loop: 'effects', finish: 'effects', modulator: 'modulators', frame: 'io' };

/** A type's category: as declared, else by its place. */
export const categoryOf = (t: ModuleType): Category => DECLARED[t.type] ?? (t.type.startsWith('music-') ? 'io' : undefined) ?? BY_PLACE[t.place] ?? 'effects';

const PICTURE = new Set(['picture', 'light']);

/** A device's category, from what it exposes: a picture in and out, an effect; out only, a source; several in, a
 *  mixer; none, a modulator. */
export function deviceCategory(ports: Ports): Category {
  const takes = ports.in.filter((p) => PICTURE.has(p.type)).length;
  const gives = ports.out.some((p) => PICTURE.has(p.type));
  if (gives && !takes) return 'sources';
  if (gives && takes > 1) return 'mixers';
  if (gives) return 'effects';
  return takes ? 'io' : 'modulators';
}

/** A patch's modules by category; the effects in the order pictures flow (a module after what feeds it; a Feedback's
 *  input, its last frame, does not count), ties and unwired ones as the patch has them. */
export function byCategory(p: PatchV2, types: Record<string, ModuleType>): ByCategory {
  const out: ByCategory = { sources: [], effects: [], mixers: [], modulators: [], io: [] };
  const ports = portsFrom(types);
  for (const m of p.modules) {
    if (m.type === 'device') {
      const d = ports(m);
      if (d) out[deviceCategory(d)].push(m);
      continue;
    }
    const t = types[m.type];
    if (t) out[categoryOf(t)].push(m);
  }
  const inLane = new Set(out.effects.map((m) => m.id));
  const before = new Map(out.effects.map((m) => [m.id, new Set<string>()]));
  for (const c of p.cables) {
    if (!inLane.has(c.from.module) || !inLane.has(c.to.module) || c.from.module === c.to.module) continue;
    before.get(c.to.module)!.add(c.from.module);
  }
  const order: Module[] = [];
  const left = [...out.effects];
  while (left.length) {
    const i = left.findIndex((m) => [...before.get(m.id)!].every((id) => order.some((x) => x.id === id)));
    order.push(...left.splice(i < 0 ? 0 : i, 1));
  }
  out.effects = order;
  return out;
}

/** A module's ports from its type, or a device's from what it exposes (ports.ts portsOfModule: the one way). */
export const portsFrom = portsOfModule;
