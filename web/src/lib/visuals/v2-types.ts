// Every module type a v2 patch may hold (decision 0057), by its type: the extras' (sources, effects, modulators), the
// Cratarr chain's (patch.ts cratarrTypes), the engine's (ports.ts: Music, the Envelope Follower, Dice, Shaper …), the
// graph's (Mix, Feedback, Screen), the picture modules (Glow, Tint …) and Show's parts. The same list as picture-graph.ts
// `v2Types`, as a record for the panel and the rack.

import { cratarrTypes, type ModuleType } from './patch';
import { ENGINE_MODULES, GRAPH_MODULES, MUSIC_SIGNAL_MODULES, OFF_ON, PICTURE_MODULES, showPartTypes, type Ports } from './ports';
import { LENSES } from './flame';
import { EXTRA_TYPES } from './registry';
import { EFFECT_TYPES } from './effect-modules';
import { DEFAULTS, RANGES } from './settings.svelte';

export type V2Type = ModuleType & { ports?: Ports };

/** The Whole frame's walks (Look's and Behaviour's columns) are not the rack's: Randoms and the Dice now (0057). */
const WALKS = new Set(['lookWalk', 'behaviourWalk']);
/** The chain's step knobs' choices by name (Knob `options`), as their help names them. */
const CHAIN_CHOICES: Record<string, readonly string[]> = {
  'flame.family': ['Spectrum', 'Arc', 'Two tones', 'Three', 'Cover', 'Fire'],
  'flame.final': ['None', ...LENSES],
  'flame.mirror': OFF_ON,
  'fluid.lattice': ['Square', 'Hex'],
};
const named = (t: ModuleType): ModuleType => ({ ...t, knobs: t.knobs.map((k) => (CHAIN_CHOICES[`${t.type}.${k.id}`] ? { ...k, options: CHAIN_CHOICES[`${t.type}.${k.id}`] } : k)) });

export function v2TypeList(chain: readonly ModuleType[] = cratarrTypes(RANGES, DEFAULTS)): V2Type[] {
  const show = chain.find((t) => t.type === 'show');
  const rack = chain.map((t) => named(t.type === 'frame' ? { ...t, knobs: t.knobs.filter((k) => !WALKS.has(k.id)) } : t));
  return [...EXTRA_TYPES, ...rack, ...ENGINE_MODULES, ...MUSIC_SIGNAL_MODULES, ...GRAPH_MODULES, ...PICTURE_MODULES, ...EFFECT_TYPES, ...(show ? showPartTypes(show) : [])];
}

/** The types by name (a later one of a name over an earlier, as picture-graph.ts's map has them). */
export const v2TypeRecord = (chain?: readonly ModuleType[]): Record<string, V2Type> => Object.fromEntries(v2TypeList(chain).map((t) => [t.type, t]));
