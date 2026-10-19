// The v1 → v2 conversion (decision 0057, step 1): today's settings and extras as a patch of modules and cables between
// typed ports, once, for the one engine. The value side follows the contract agreed with claude-5f, so the engine on
// a converted patch gives what aspects.ts gives on the settings:
//
// 1. Today's chain keeps its module ids (frame, flame, show, fluid, tunnel, dive, trails, loop-effect, fx1–3) and its
//    knobs (patch.ts SETTING_KNOB, ASPECT_KNOB); a column's scale is its knob's own value.
// 2. A route from the loudness comes from the Loudness module's `level` (the loudness as Intensity lets it count), and
//    is never scaled by it (`depth: null`).
// 3. A route from the track comes from Track's `pulse`.
// 4. Every other route comes from Music's port of its signal (a band's as that channel), read as the routes read it
//    (`reference: 'absolute'`), its depth normalled to the Loudness's `out`.
// 5. A route's cable keeps the route's key as its id, its value (an amount or a curve, walking or not) and its mode.
// 6. Intensity, Flash and Speed move to the modules that do them: Loudness, Flash, Clock.
// 7. The extras' modules come as they are; their cables from Music's or a modulator's port.
//
// The picture side's wiring (which picture goes into which) is claude-632's (step 3): `pictureCables`, empty until
// theirs lands.

import { isRoute } from './routes';
import { cratarrPatch, modulesOf, type Cable, type Module, type Patch } from './patch';
import type { CableV2, PatchV2 } from './ports';
import type { VisualSettings } from './settings.svelte';

const BANDS = 6;

/** The settings that leave the frame for the module that does them (6). */
const MOVED: Record<string, [string, string]> = { intensity: ['envelope', 'intensity'], flash: ['flash', 'flash'], speed: ['clock', 'speed'] };

/** Where a route's signal comes from (2–4). */
function routeFrom(c: Cable): Pick<CableV2, 'from' | 'depth'> {
  const from = c.from as { signal: string; band?: number };
  if (from.signal === 'loudness' && from.band === undefined) return { from: { module: 'envelope', port: 'level' }, depth: null };
  if (from.signal === 'track') return { from: { module: 'track', port: 'pulse' } };
  return { from: { module: 'music', port: from.signal, ...(from.band !== undefined ? { channel: from.band } : {}), reference: 'absolute' } };
}

/** Where an extra cable comes from (7): a modulator's output, or Music's port with the cable's own reference and speed. */
function extraFrom(c: Cable): CableV2['from'] {
  if ('modulator' in c.from) return { module: c.from.modulator, port: 'out' };
  const f = c.from;
  return {
    module: 'music',
    port: f.signal,
    ...(f.band !== undefined ? { channel: f.band } : {}),
    ...(f.reference ? { reference: f.reference } : {}),
    ...(f.speed ? { speed: f.speed } : {}),
  };
}

/** The picture side's cables for a patch (claude-632's, step 3); none until theirs is in. */
export let pictureCables: (p: Patch) => CableV2[] = () => [];
export function setPictureCables(f: (p: Patch) => CableV2[]) {
  pictureCables = f;
}

/** Today's settings (with their extras) as a v2 patch. */
export function toV2(s: VisualSettings): PatchV2 {
  const p = cratarrPatch(s);
  const modules: Module[] = [...modulesOf(p).values()].map((m) => ({ ...m, knobs: { ...m.knobs } }));
  const frame = modules.find((m) => m.id === 'frame');
  const engine = new Map<string, Module>([
    ['music', { id: 'music', type: 'music', knobs: {} }],
    ['track', { id: 'track', type: 'track', knobs: {} }],
    ['envelope', { id: 'envelope', type: 'envelope', knobs: { time: 0.2 } }],
    ['clock', { id: 'clock', type: 'clock', knobs: { beats: 32 } }],
    ['flash', { id: 'flash', type: 'flash', knobs: { course: 0.3 } }],
  ]);
  for (const [setting, [module, knob]] of Object.entries(MOVED)) {
    engine.get(module)!.knobs[knob] = (s as unknown as Record<string, number>)[setting];
    if (frame) delete frame.knobs[setting];
  }
  const extraIds = new Set((s.extra?.cables ?? []).map((c) => c.id));
  const cables: CableV2[] = [];
  for (const c of p.cables) {
    const to = { module: c.to.module, port: c.to.knob };
    if (isRoute(c.id, BANDS) && !extraIds.has(c.id)) {
      // A route set to nothing is no cable.
      if (typeof c.value === 'number' && c.value === 0) continue;
      cables.push({ id: c.id, ...routeFrom(c), to, value: c.value, mode: c.mode });
    } else cables.push({ id: c.id, from: extraFrom(c), to, value: c.value, mode: c.mode });
  }
  cables.push(...pictureCables(p));
  return { v: 2, modules: [...engine.values(), ...modules], cables, macros: p.macros, pins: p.pins };
}
