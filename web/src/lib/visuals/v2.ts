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
// 6. Intensity, Flash and Speed move to the modules that do them: Loudness, Accent (Flash renamed, 0057 §7e), Clock. Show is split into its
//    device's modules (ports.ts SHOW_PARTS), its knobs exposed under their old names, so a route into
//    `show.<knob>` keeps its address (`flatten` leads it to the module behind); Show's Size is the Flame's `zoom`.
// 7. The extras' modules come as they are; their cables from Music's or a modulator's port.
//
// The picture side's wiring (which picture goes into which, and the modules it needs: the Screen, the Feedbacks, the
// Mixes) is claude-632's (step 3): `pictureWiring`, none until theirs lands.

import type { RouteValue } from './curves';
import { ASPECTS, BAND_SIGNALS, COLUMN_SCALES, isRoute, LEVEL_ASPECTS, MOMENT_ASPECTS, PART_ASPECTS, type Aspect } from './routes';
import { shapeOf } from './evaluate';
import { DEFAULT_LIMITS, type Budgeted } from './budget';
import { BUSY } from './engine/sidechain';
import { ASPECT_KNOB, cratarrPatch, modulesOf, type Cable, type Macro, type Module, type Patch } from './patch';
import { isDevice, READINGS, SHOW_PARTS, type CableV2, type Device, type PatchV2, type PortRef } from './ports';
import { extraType } from './registry';

/** A cable as the conversion carries it until the end: how it reads its signal, its speed and its mode still on it; they
 *  leave the wire at the end (`toWire`). */
type Wire = Omit<CableV2, 'from'> & { from: PortRef & { reference?: (typeof READINGS)[number]; speed?: 'fast' | 'slow' }; mode: 'level' | 'swing' };
import type { VisualSettings } from './settings.svelte';

const BANDS = 6;

/** The settings that leave the frame for the module that does them (6). */
const MOVED: Record<string, [string, string]> = { intensity: ['envelope', 'intensity'], flash: ['accent', 'accent'], speed: ['clock', 'speed'] };

/** Where a route's signal comes from (2–4). */
function routeFrom(c: Cable): Pick<Wire, 'from' | 'depth'> {
  const from = c.from as { signal: string; band?: number };
  if (from.signal === 'loudness' && from.band === undefined) return { from: { module: 'envelope', port: 'level' }, depth: null };
  if (from.signal === 'track') return { from: { module: 'track', port: 'pulse' } };
  return { from: { module: 'music', port: from.signal, ...(from.band !== undefined ? { channel: from.band } : {}), reference: 'absolute' } };
}

/** Where an extra cable comes from (7): a modulator's output, or Music's port with the cable's own reference and speed. */
function extraFrom(c: Cable): Wire['from'] {
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
export type Wiring = { modules: Module[]; cables: Wire[] };
export let pictureWiring: (p: Patch) => Wiring = () => ({ modules: [], cables: [] });
export function setPictureWiring(f: (p: Patch) => Wiring) {
  pictureWiring = f;
}

/** The Show device: Bend, Blur, Tone map, Curve and Saturate in a row, light in and a picture out, each knob of Show's
 *  (but Size) exposed under its own name. */
function showDevice(show: Module): Device {
  const parts = SHOW_PARTS.map(({ knob, type }) => ({ id: type, type, knobs: { ...(knob in show.knobs ? { [knob]: show.knobs[knob] } : {}), ...(type === 'tonemap' ? { level: 1 } : {}) } }));
  const wire = (i: number): CableV2 => ({
    id: `show-${i}`,
    from: { module: SHOW_PARTS[i].type, port: 'picture' },
    to: { module: SHOW_PARTS[i + 1].type, port: SHOW_PARTS[i + 1].type === 'tonemap' ? 'light' : 'picture' },
    value: 1,
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
      // Tone map's Level and its gain (0057 §7e item 6: the pause's dimming).
      { as: 'level', label: 'Level', module: 'tonemap', port: 'level' },
      { as: 'level:gain', label: 'Level: gain', module: 'tonemap', port: 'level:gain' },
      // Its level knobs' gains (0057 §7e item 1), for the VCAs cables now go through.
      ...SHOW_PARTS.filter(({ knob }) => LEVEL_KNOBS.has(`show.${knob}`)).map(({ knob, type, label }) => ({ as: `${knob}:gain`, label: `${label}: gain`, module: type, port: `${knob}:gain` })),
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
  const cables: Wire[] = [];
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

/** A cable's value scaled (0057 §7e item 2): an amount times `s`, a curve's heights times `s`. */
function scaled(v: RouteValue, s: number): RouteValue {
  if (s === 1) return v;
  if (typeof v === 'number') return v * s;
  return { ...v, h1: [v.h1[0], v.h1[1] * s], h2: [v.h2[0], v.h2[1] * s], h: v.h * s };
}

/** The column scales (a knob that also scaled every cable into it) folded into each cable's own amount (0057 §7e item 2;
 *  claude-5f's shape): by column knob, the setting that scales it. The fluid's stir, flow and swirl are scaled where
 *  it stirs, not here. */
const SCALE_OF = new Map(
  (Object.entries(COLUMN_SCALES) as [string, string][]).filter(([a]) => a !== 'stir' && a !== 'flow' && a !== 'swirl').map(([a, key]) => [ASPECT_KNOB[a as keyof typeof ASPECT_KNOB].join('.'), key]),
);

/** Each column knob's aspect (today's chain), by `module.knob`. */
const ASPECT_AT = new Map((Object.entries(ASPECT_KNOB) as [Aspect, [string, string]][]).map(([a, [m, k]]) => [`${m}.${k}`, a]));
const LEVEL_KNOBS = new Set([...ASPECT_AT].filter(([, a]) => LEVEL_ASPECTS.includes(a)).map(([k]) => k));

/** Cables only add (0057 §7e items 1, 3, 4; claude-5f's shapes, proved frame by frame over the recorded tracks):
 *  - a level column's cables each into a VCA's cv (its peak taken off, by the Loudness's out or its own depth), the
 *    VCAs in series, the bands' first and through a Merge (geometric mean), the last into the knob's gain;
 *  - a moment column's cables into a Merge (largest), then a Beat gate, then the step's next;
 *  - a band's cables into another column (not a part's: the parts keep their channels) through a Merge (mean). */
function cablesAdd(modules: Module[], cables: Wire[]): { modules: Module[]; cables: Wire[] } {
  const out: Wire[] = [];
  const added: Module[] = [];
  const byColumn = new Map<string, Wire[]>();
  // A knob of the extras' modules: its kind (a modulator's port is none).
  const typeOf = new Map(modules.map((m) => [m.id, m.type]));
  const kindAt = (module: string, knob: string) => extraType(typeOf.get(module) ?? '')?.knobs.find((k) => k.id === knob);
  const shapeAt = new Map<string, 'level' | 'moment' | 'mean'>();
  // A cable carrying the bands: one band of them (its channel), or all six at once (from a band's port with no channel:
  // Music's band rows, an ADSR).
  const banded = (c: Wire) => c.from.channel !== undefined || (c.from.module === 'music' && (BAND_SIGNALS as readonly string[]).includes(c.from.port.replace(/:slow$/, ''))) || typeOf.get(c.from.module) === 'adsr';
  for (const c of cables) {
    const key = `${c.to.module}.${c.to.port}`;
    const aspect = ASPECT_AT.get(key);
    const k = aspect ? undefined : kindAt(c.to.module, c.to.port);
    const level = aspect ? LEVEL_ASPECTS.includes(aspect) : !!k && c.mode === 'level';
    const moment = aspect ? (MOMENT_ASPECTS as readonly string[]).includes(aspect) : k?.kind === 'step';
    const merged = !level && !moment && banded(c) && (aspect ? !PART_ASPECTS.includes(aspect) : !!k && !k.channels);
    if (level || moment || merged) {
      shapeAt.set(key, level ? 'level' : moment ? 'moment' : 'mean');
      (byColumn.get(key) ?? byColumn.set(key, []).get(key)!).push(c);
    } else out.push(c);
  }
  for (const [key, cs] of byColumn) {
    const [module, knob] = key.split('.');
    const col = `${module}-${knob}`;
    const shape = shapeAt.get(key)!;
    const one = (from: string, to: Wire['to'], id: string): Wire => ({ id, from: { module: from, port: 'out' }, to, value: 1, mode: 'swing', depth: null });
    if (shape === 'level') {
      let prev: string | null = null;
      let i = 0;
      const vca = (c: Wire) => {
        const id = `vca-${col}-${i++}`;
        const peak = shapeOf(c.value).peak;
        const ch = c.from.channel !== undefined ? { channel: c.from.channel } : {};
        added.push({ id, type: 'vca', knobs: { gain: c.depth === null ? 1 - peak : 1 } });
        if (prev) out.push(one(prev, { module: id, port: 'in' }, `${id}-in`));
        out.push({ ...c, mode: 'swing', to: { module: id, port: 'cv', ...ch } });
        if (c.depth !== null) out.push({ id: `${id}-peak`, from: c.depth ?? { module: 'envelope', port: 'out' }, to: { module: id, port: 'cv', ...ch }, value: -peak, mode: 'swing', depth: null });
        prev = id;
      };
      const bands = cs.filter((c) => c.from.channel !== undefined);
      for (const c of bands) vca(c);
      if (bands.length) {
        added.push({ id: `merge-${col}`, type: 'merge', knobs: { mode: 3 } });
        out.push(one(prev!, { module: `merge-${col}`, port: 'in' }, `merge-${col}-in`));
        prev = `merge-${col}`;
      }
      for (const c of cs.filter((c) => c.from.channel === undefined)) vca(c);
      out.push(one(prev!, { module, port: `${knob}:gain` }, `${col}-gain`));
    } else if (shape === 'moment') {
      added.push({ id: `merge-${col}`, type: 'merge', knobs: { mode: 2 } });
      for (const c of cs) out.push({ ...c, to: { module: `merge-${col}`, port: 'in', ...(c.from.channel !== undefined ? { channel: c.from.channel } : {}) } });
      added.push({ id: `gate-${col}`, type: 'gate', knobs: { above: 0.5, hysteresis: 1 / 3, lift: 0, recover: 0, release: 0, gap: 2, amount: 1 } });
      out.push(one(`merge-${col}`, { module: `gate-${col}`, port: 'in' }, `gate-${col}-in`));
      out.push(one(`gate-${col}`, { module, port: `${knob}:next` }, `${col}-next`));
    } else {
      added.push({ id: `mean-${col}`, type: 'merge', knobs: { mode: 1 } });
      for (const c of cs) out.push({ ...c, to: { module: `mean-${col}`, port: 'in', channel: c.from.channel } });
      out.push(one(`mean-${col}`, { module, port: knob }, `mean-${col}-out`));
    }
  }
  return { modules: [...modules, ...added], cables: out };
}

/** The sidechain in sight (0057 §7e; claude-5f's wiring, proved against today's levers): a Meter on the picture, a
 *  Ducker a kind keyed by it (its threshold the limit as learned and frozen, × Wild's factor, × the music's activity
 *  through a VCA from the Loudness's level), each into the gain of what that kind of busyness moves; the Screen's flash
 *  guard into the Accent's gain. */
const SIDECHAIN: readonly { kind: string; meter: Budgeted; into: readonly [string, string] | null }[] = [
  { kind: 'motion', meter: 'motion', into: ['clock', 'speed'] },
  { kind: 'shape', meter: 'shapeChurn', into: ['flame', 'evolution'] },
  { kind: 'colour', meter: 'colourChurn', into: ['flame', 'hue'] },
  { kind: 'zoom', meter: 'zoomPumping', into: ['flame', 'zoom'] },
  // The moments' Gates (null): their gaps stretched.
  { kind: 'looks', meter: 'looksPerMinute', into: null },
];
function sidechained(modules: Module[], cables: Wire[], wild: number, limits: Partial<Record<Budgeted, number>>): { modules: Module[]; cables: Wire[]; macro?: Macro } {
  const one = (id: string, from: Wire['from'], to: Wire['to'], value = 1): Wire => ({ id, from, to, value, mode: 'swing', depth: null });
  const out = { modules: [...modules, { id: 'meter', type: 'meter', knobs: {} }], cables: [...cables] };
  if (!out.modules.some((m) => m.type === 'screen')) out.modules.push({ id: 'screen', type: 'screen', knobs: {} });
  out.cables.push(one('guard', { module: 'screen', port: 'guard' }, { module: 'accent', port: 'accent:gain' }));
  const wildFactor = 2 ** (2 * (wild - 0.5));
  // Wild, a macro on the five thresholds (the owner): along the threshold's exponential taper from half to twice
  // (× 1.5) of the limit, so at today's Wild it is exactly today's factor 2^(2(wild − ½)).
  const macro: Macro = { id: 'wild', on: 'frame', label: 'Wild', value: wild, targets: [] };
  for (const { kind, meter, into } of SIDECHAIN) {
    const id = `duck-${kind}`;
    const limit = limits[meter] ?? DEFAULT_LIMITS[meter];
    macro.targets.push({ module: id, knob: 'threshold', from: limit * 1.5 * 0.5, to: limit * 1.5 * 2 });
    out.modules.push({ id, type: 'ducker', knobs: { threshold: limit * wildFactor * 1.5, depth: Math.abs(20 * Math.log10(BUSY[meter].most)), attack: 1 / 0.7, hold: 0, release: 10 / 3, hysteresis: 0.8 } });
    out.cables.push(one(`${id}-key`, { module: 'meter', port: kind }, { module: id, port: 'key' }));
    out.modules.push({ id: `${id}-activity`, type: 'vca', knobs: { gain: 1 / 3 } });
    out.cables.push(one(`${id}-loudness`, { module: 'envelope', port: 'level' }, { module: `${id}-activity`, port: 'cv' }, 2 / 3));
    out.cables.push(one(`${id}-activity-out`, { module: `${id}-activity`, port: 'out' }, { module: id, port: 'threshold:gain' }));
    const targets = into ? [into] : modules.filter((m) => m.type === 'gate' && m.id.startsWith('gate-')).map((m) => [m.id, 'gap'] as const);
    for (const [module, knob] of targets) out.cables.push(one(`${id}-${module}`, { module: id, port: 'out' }, { module, port: `${knob}:gain` }));
  }
  return { ...out, macro };
}

/** The cables as wires (0057 §7e item 10): how each signal is read goes to its Music row (`reference.<signal>`: 'absolute'
 *  wherever a route reads the row, as routes do, else what its cables read; a cable reading otherwise keeps its row's,
 *  rare), and an ADSR's gate's to that ADSR; a slow reading to Music's `<signal>:slow`; the mode gone (cables only add,
 *  the levels already through VCAs). */
function toWire(modules: Module[], wires: Wire[]): CableV2[] {
  const music = modules.find((m) => m.id === 'music');
  const reading = new Map<string, (typeof READINGS)[number]>();
  for (const w of wires) {
    if (w.from.module !== 'music' || modules.find((m) => m.id === w.to.module)?.type === 'adsr') continue;
    const r = w.from.reference ?? 'track';
    const was = reading.get(w.from.port);
    if (!was || r === 'absolute') reading.set(w.from.port, r);
  }
  if (music) for (const [signal, r] of reading) if (r !== 'track') music.knobs[`reference.${signal}`] = READINGS.indexOf(r);
  return wires.map(({ mode: _mode, ...w }) => {
    const { reference, speed, ...from } = w.from;
    const adsr = modules.find((m) => m.id === w.to.module && m.type === 'adsr');
    if (adsr && from.module === 'music') adsr.knobs.reference = READINGS.indexOf(reference ?? 'track');
    const slow = speed === 'slow' && from.module === 'music' ? { port: `${from.port}:slow` } : {};
    return { ...w, from: { ...from, ...slow } };
  });
}

/** Today's settings (with their extras) as a v2 patch.
 *  - `addOnly` (default true): cables only add (0057 §7e); false gives the patch before that step, for the proof.
 *  - `sidechain` (default true): the sidechain in sight; false leaves it out, for the proof.
 *  - `limits`: the limits as learned (the app's `limitsNow`), frozen into the Duckers; else the defaults. */
export function toV2(s: VisualSettings, opts: { addOnly?: boolean; sidechain?: boolean; limits?: Partial<Record<Budgeted, number>> } = {}): PatchV2 {
  const p = cratarrPatch(s);
  // Drift is Random now (its smooth end: Shape 0), as VCV, Vital and Bitwig name it (the owner).
  const modules: Module[] = [...modulesOf(p).values()].map((m) => (m.type === 'drift' ? { ...m, type: 'random', knobs: { ...m.knobs, shape: 0 } } : { ...m, knobs: { ...m.knobs } }));
  const frame = modules.find((m) => m.id === 'frame');
  const engine = new Map<string, Module>([
    ['music', { id: 'music', type: 'music', knobs: {} }],
    ['track', { id: 'track', type: 'track', knobs: {} }],
    ['envelope', { id: 'envelope', type: 'envelope', knobs: { time: 0.2 } }],
    ['clock', { id: 'clock', type: 'clock', knobs: { beats: 32 } }],
    ['accent', { id: 'accent', type: 'accent', knobs: { course: 0.3 } }],
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
  const target = (to: Cable['to']) => ({ module: to.module, port: to.knob });
  // The bands' Linger leaves the Flame: Music's hold, and the ADSRs' release.
  const lighting = [...s.lighting];
  for (let b = 0; b < BANDS; b++) {
    engine.get('music')!.knobs[`hold.${b}`] = lighting[b];
    if (flame) delete flame.knobs[`lighting.${b}`];
  }
  // A cable from the hits comes from their ADSR (the band's channel, or all of them). How the hits are read (their
  // reference and speed: a route's as it is, an extra cable's against the track's typical by default) goes on the
  // ADSR's gate cable, so a reading of its own gets an ADSR of its own (`adsr`, `adsr-track`, `adsr-drums-lately` …).
  const adsrs = new Map<string, { module: Module; gate: Wire }>();
  const viaAdsr = (from: Wire['from']): Wire['from'] => {
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
  const cables: Wire[] = [];
  // A column's scale goes into its cables; its knob then scales nothing (1, until the engine drops SCALED).
  const scaleOf = (to: Cable['to']) => {
    const key = SCALE_OF.get(`${to.module}.${to.knob}`);
    return key ? ((s as unknown as Record<string, number>)[key] ?? 1) : 1;
  };
  for (const key of SCALE_OF.keys()) {
    const [mid, kid] = key.split('.');
    // Show's are inside its device (below); its size is the Flame's zoom.
    const m = mid === 'show' ? undefined : modules.find((x) => x.id === mid);
    if (m) m.knobs[kid] = 1;
  }
  const showAsDevice = modules.find((x) => x.id === 'show') as Device | undefined;
  for (const part of showAsDevice?.patch.modules ?? []) for (const k of Object.keys(part.knobs)) if (SCALE_OF.has(`show.${k}`)) part.knobs[k] = 1;
  for (const c of p.cables) {
    const to = target(c.to);
    const value = scaled(c.value, scaleOf(c.to));
    if (isRoute(c.id, BANDS) && !extraIds.has(c.id)) {
      // A route set to nothing is no cable.
      if (typeof c.value === 'number' && c.value === 0) continue;
      const r = routeFrom(c);
      cables.push({ id: c.id, ...r, from: viaAdsr(r.from), to, value, mode: c.mode });
    } else cables.push({ id: c.id, from: viaAdsr(extraFrom(c)), to, value, mode: c.mode });
  }
  for (const a of adsrs.values()) cables.push(a.gate);
  // Pausing dims in sight (0057 §7e item 6, claude-5f's): Music's playing, followed over half a second, into a VCA
  // (0.55 paused, 1 playing) into Tone map's Level gain.
  const pause: Module[] = [
    { id: 'pause-follow', type: 'slow', knobs: { attack: 0.5, release: 0.5, full: 1, amount: 1 } },
    { id: 'pause-dim', type: 'vca', knobs: { gain: 0.55 } },
  ];
  cables.push(
    { id: 'pause-playing', from: { module: 'music', port: 'playing' }, to: { module: 'pause-follow', port: 'in' }, value: 1, mode: 'swing', depth: null },
    { id: 'pause-follow-out', from: { module: 'pause-follow', port: 'out' }, to: { module: 'pause-dim', port: 'cv' }, value: 0.45, mode: 'swing', depth: null },
    { id: 'pause-dim-out', from: { module: 'pause-dim', port: 'out' }, to: { module: 'show', port: 'level:gain' }, value: 1, mode: 'swing', depth: null },
  );
  const all = [...engine.values(), ...[...adsrs.values()].map((a) => a.module), ...modules, ...pause];
  const adding = opts.addOnly === false ? { modules: all, cables } : cablesAdd(all, cables);
  const added = opts.sidechain === false ? adding : sidechained(adding.modules, adding.cables, s.wild, opts.limits ?? {});
  const wiring = pictureWiring(p);
  const macros = 'macro' in added && added.macro ? [...p.macros, added.macro] : p.macros;
  return { v: 2, modules: [...added.modules, ...wiring.modules], cables: [...toWire(added.modules, added.cables), ...wiring.cables], macros, pins: p.pins };
}
