// Every module type a v2 patch may hold (decision 0057), by its type: the extras' (sources, effects, modulators), the
// Cratarr chain's (patch.ts cratarrTypes), the engine's (ports.ts: Music, the Envelope Follower, Dice, Shaper …), the
// graph's (Mix, Feedback, Screen), the picture modules (Glow, Tint …) and Show's parts. The same list as picture-graph.ts
// `v2Types`, as a record for the panel and the rack.

import { cratarrTypes, type ModuleType } from './patch';
import { ENGINE_MODULES, GRAPH_MODULES, PICTURE_MODULES, showPartTypes, type Ports } from './ports';
import { EXTRA_TYPES } from './registry';
import { DEFAULTS, RANGES } from './settings.svelte';

export type V2Type = ModuleType & { ports?: Ports };

export function v2TypeList(chain: readonly ModuleType[] = cratarrTypes(RANGES, DEFAULTS)): V2Type[] {
  const show = chain.find((t) => t.type === 'show');
  return [...EXTRA_TYPES, ...chain, ...ENGINE_MODULES, ...GRAPH_MODULES, ...PICTURE_MODULES, ...(show ? showPartTypes(show) : [])];
}

/** The types by name (a later one of a name over an earlier, as picture-graph.ts's map has them). */
export const v2TypeRecord = (chain?: readonly ModuleType[]): Record<string, V2Type> => Object.fromEntries(v2TypeList(chain).map((t) => [t.type, t]));
