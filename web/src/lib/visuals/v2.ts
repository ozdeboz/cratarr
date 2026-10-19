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
// 6. Intensity, Flash and Speed move to the modules that do them: Loudness, Flash, Clock. Show is split into its
//    device's modules (ports.ts SHOW_PARTS), its knobs exposed under their old names, so a route into
//    `show.<knob>` keeps its address (`flatten` leads it to the module behind); Show's Size is the Flame's `zoom`.
// 7. The extras' modules come as they are; their cables from Music's or a modulator's port.
//
// The picture side's wiring (which picture goes into which, and the modules it needs: the Screen, the Feedbacks, the
// Mixes) is claude-632's (step 3): `pictureWiring`, none until theirs lands.

import { isRoute } from './routes';
import { cratarrPatch, modulesOf, type Cable, type Module, type Patch } from './patch';
import { isDevice, SHOW_PARTS, type CableV2, type Device, type PatchV2, type PortRef } from './ports';
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
export type Wiring = { modules: Module[]; cables: CableV2[] };
export let pictureWiring: (p: Patch) => Wiring = () => ({ modules: [], cables: [] });
export function setPictureWiring(f: (p: Patch) => Wiring) {
  pictureWiring = f;
}

/** The Show device: Bend, Blur, Tone map, Curve and Saturate in a row, light in and a picture out, each knob of Show's
 *  (but Size) exposed under its own name. */
function showDevice(show: Module): Device {
  const parts = SHOW_PARTS.map(({ knob, type }) => ({ id: type, type, knobs: knob in show.knobs ? { [knob]: show.knobs[knob] } : {} }));
  const wire = (i: number): CableV2 => ({
    id: `show-${i}`,
    from: { module: SHOW_PARTS[i].type, port: 'picture' },
    to: { module: SHOW_PARTS[i + 1].type, port: SHOW_PARTS[i + 1].type === 'tonemap' ? 'light' : 'picture' },
    value: 1,
    mode: 'swing',
  });
  return {
    id: 'show',
    type: 'device',
    label: 'Show',
    knobs: {},
    patch: { v: 2, modules: parts, cables: SHOW_PARTS.slice(1).map((_, i) => wire(i)), macros: [], pins: [] },
    inputs: [
      { as: 'light', label: 'Light', module: SHOW_PARTS[0].type, port: 'picture' },
      ...SHOW_PARTS.map(({ knob, type, label }) => ({ as: knob, label, module: type, port: knob })),
    ],
    outputs: [{ as: 'picture', label: 'Picture', module: SHOW_PARTS[SHOW_PARTS.length - 1].type, port: 'picture' }],
    macros: [],
  };
}

/** A patch with its devices opened up (0057 §6), for the engine and the compositor: each device's modules under
 *  `<device>/<module>`, its own cables inside, and the cables to and from its exposed ports led to the modules behind
 *  them. Devices in devices too. */
export function flatten(p: PatchV2): PatchV2 {
  return opened(p).patch;
}

/** `p` flattened, and where each of its devices' exposed ports leads (by `device.port`), through nested ones
 *  (claude-632's fix). */
function opened(p: PatchV2): { patch: PatchV2; behind: Map<string, PortRef> } {
  const modules: Module[] = [];
  const cables: CableV2[] = [];
  const behind = new Map<string, PortRef>();
  for (const m of p.modules) {
    if (!isDevice(m)) {
      modules.push(m);
      continue;
    }
    const inner = opened(m.patch);
    const at = (id: string) => `${m.id}/${id}`;
    modules.push(...inner.patch.modules.map((x) => ({ ...x, id: at(x.id) })));
    cables.push(...inner.patch.cables.map((c) => ({ ...c, id: at(c.id), from: { ...c.from, module: at(c.from.module) }, to: { ...c.to, module: at(c.to.module) } })));
    for (const e of [...m.inputs, ...m.outputs]) {
      const r = inner.behind.get(`${e.module}.${e.port}`) ?? e;
      behind.set(`${m.id}.${e.as}`, { module: at(r.module), port: r.port, ...(e.channel !== undefined ? { channel: e.channel } : {}) });
    }
  }
  const lead = <T extends PortRef>(r: T): T => {
    const b = behind.get(`${r.module}.${r.port}`);
    return b ? { ...r, module: b.module, port: b.port } : r;
  };
  for (const c of p.cables) cables.push({ ...c, from: lead(c.from), to: lead(c.to) });
  return { patch: { ...p, modules, cables }, behind };
}

/** The bands' hits through an ADSR (claude-5f's mapping of today's Linger): `adsr` for the hits, `adsr-drums` for the
 *  percussive hits, made where a cable needs one; release a band its lighting time, attack 0.04 s, no decay or
 *  sustain, a hit starting from the higher of the two (as today's lights). */
const ADSR_OF: Record<string, string> = { hits: 'adsr', percussiveHits: 'adsr-drums' };
function adsrModule(id: string, lighting: number[]): Module {
  const knobs: Record<string, number> = { retrigger: 0, amount: 1 };
  for (let b = 0; b < BANDS; b++) Object.assign(knobs, { [`attack.${b}`]: 0.04, [`decay.${b}`]: 0, [`sustain.${b}`]: 0, [`release.${b}`]: lighting[b] });
  return { id, type: 'adsr', knobs };
}

/** Today's settings (with their extras) as a v2 patch. */
export function toV2(s: VisualSettings): PatchV2 {
  const p = cratarrPatch(s);
  // Drift is Random now (its smooth end: Shape 0), as VCV, Vital and Bitwig name it (the owner).
  const modules: Module[] = [...modulesOf(p).values()].map((m) => (m.type === 'drift' ? { ...m, type: 'random', knobs: { ...m.knobs, shape: 0 } } : { ...m, knobs: { ...m.knobs } }));
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
  // Show as a device (0057 §6): its modules in the order that draws today's picture, each with the knob it took; its
  // knobs exposed under their old names, so the routes into `show.<knob>` keep their address. Its Size is the Flame's
  // own zoom.
  const showAt = modules.findIndex((m) => m.id === 'show');
  if (showAt >= 0) modules.splice(showAt, 1, showDevice(modules[showAt]));
  const flame = modules.find((m) => m.id === 'flame');
  const show = p.layers[0]?.modules.find((m) => m.id === 'show');
  if (flame && show && typeof show.knobs.size === 'number') flame.knobs.zoom = show.knobs.size;
  const target = (to: Cable['to']) => (to.module === 'show' && to.knob === 'size' ? { module: 'flame', port: 'zoom' } : { module: to.module, port: to.knob });
  // The bands' Linger leaves the Flame: Music's hold, and the ADSRs' release.
  const lighting = [...s.lighting];
  for (let b = 0; b < BANDS; b++) {
    engine.get('music')!.knobs[`hold.${b}`] = lighting[b];
    if (flame) delete flame.knobs[`lighting.${b}`];
  }
  // A cable from the hits comes from their ADSR (the band's channel, or all of them). How the hits are read (their
  // reference and speed: a route's as it is, an extra cable's against the track's typical by default) goes on the
  // ADSR's gate cable, so a reading of its own gets an ADSR of its own (`adsr`, `adsr-track`, `adsr-drums-lately` …).
  const adsrs = new Map<string, { module: Module; gate: CableV2 }>();
  const viaAdsr = (from: CableV2['from']): CableV2['from'] => {
    const base = from.module === 'music' ? ADSR_OF[from.port] : undefined;
    if (!base) return from;
    const reference = from.reference ?? 'track';
    const speed = from.speed;
    const id = [base, reference === 'absolute' ? null : reference, speed ?? null].filter(Boolean).join('-');
    if (!adsrs.has(id))
      adsrs.set(id, {
        module: adsrModule(id, lighting),
        gate: { id: `${id}-gate`, from: { module: 'music', port: from.port, reference, ...(speed ? { speed } : {}) }, to: { module: id, port: 'gate' }, value: 1, mode: 'swing' },
      });
    return { module: id, port: 'out', ...(from.channel !== undefined ? { channel: from.channel } : {}) };
  };
  const extraIds = new Set((s.extra?.cables ?? []).map((c) => c.id));
  const cables: CableV2[] = [];
  for (const c of p.cables) {
    const to = target(c.to);
    if (isRoute(c.id, BANDS) && !extraIds.has(c.id)) {
      // A route set to nothing is no cable.
      if (typeof c.value === 'number' && c.value === 0) continue;
      const r = routeFrom(c);
      cables.push({ id: c.id, ...r, from: viaAdsr(r.from), to, value: c.value, mode: c.mode });
    } else cables.push({ id: c.id, from: viaAdsr(extraFrom(c)), to, value: c.value, mode: c.mode });
  }
  for (const a of adsrs.values()) cables.push(a.gate);
  const wiring = pictureWiring(p);
  cables.push(...wiring.cables);
  return { v: 2, modules: [...engine.values(), ...[...adsrs.values()].map((a) => a.module), ...modules, ...wiring.modules], cables, macros: p.macros, pins: p.pins };
}
