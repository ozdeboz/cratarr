// The presets (decision 0057, the switch): v2 patches to start from. The owner's saved looks converted as they are
// and pruned (presets/*.json: Default, Entrance to Zanzibar, Glitch Adventure), the chains and the examples (chains.ts,
// converted when chosen), and Start from scratch: nothing at all.

import { chainRoutes, CHAINS, EXAMPLES } from './chains';
import type { PatchV2 } from './ports';
import { BANDS, DEFAULTS, type VisualSettings } from './settings.svelte';
import { toV2 } from './v2';
import './picture-wiring';
import defaultPatch from './presets/default.json' with { type: 'json' };
import zanzibar from './presets/entrance-to-zanzibar.json' with { type: 'json' };
import glitch from './presets/glitch-adventure.json' with { type: 'json' };

export type Preset = { id: string; label: string; line: string; patch: () => PatchV2 };

const saved = (id: string, label: string, line: string, p: unknown): Preset => ({ id, label, line, patch: () => structuredClone(p as PatchV2) });

export const PRESETS: readonly Preset[] = [
  saved('default', 'Default', 'The visuals as they first came', defaultPatch),
  saved('entrance-to-zanzibar', 'Entrance to Zanzibar', 'Saved', zanzibar),
  saved('glitch-adventure', 'Glitch Adventure', 'Saved', glitch),
  ...[...CHAINS, ...EXAMPLES].map((c) => ({
    id: c.id,
    label: c.label,
    line: c.line,
    patch: () => toV2({ ...structuredClone(DEFAULTS), ...(c.settings ?? {}), routes: chainRoutes(c, BANDS), extra: structuredClone(c.extra) } as VisualSettings),
  })),
  { id: 'scratch', label: 'Start from scratch', line: 'Nothing: add a source', patch: () => ({ v: 2, modules: [], cables: [], macros: [], pins: [] }) },
];
