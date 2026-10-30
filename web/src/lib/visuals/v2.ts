// The v1 → v2 conversion (decision 0057, step 1): today's settings and extras as a patch of modules and cables between
// typed ports, once, for the one engine. The value side follows the contract agreed for the engine, so the engine on
// a converted patch gives what aspects.ts gives on the settings:
//
// 1. Today's chain keeps its module ids (flame, show, fluid, tunnel, dive, trails; the effects their modules') and its
//    knobs (patch.ts SETTING_KNOB, ASPECT_KNOB); a column's scale is its knob's own value.
// 2. A route from the loudness comes from the Loudness module's `level` (the loudness as Intensity lets it count), and
//    is never scaled by it (`depth: null`).
// 3. A route from the track comes from an ADSR on Music's `newtrack` (Track folded into Music, 0057 §7f).
// 4. Every other route comes from Music's port of its signal (a band's as that channel), read as the routes read it
//    (`reference: 'absolute'`), its depth normalled to the Loudness's `out`.
// 5. A route's cable keeps the route's key as its id, its value (an amount or a curve, walking or not) and its mode.
// 6. Intensity, Flash and Speed move to the modules that do them: the Loudness (an Envelope Follower), Accent (Flash
//    renamed, 0057 §7e), Clock; each a rack macro on that knob, as Wild is on the Duckers'. Show is split into its
//    device's modules (ports.ts SHOW_PARTS), its knobs exposed under their old names, so a route into
//    `show.<knob>` keeps its address (`flatten` leads it to the module behind); Show's Size is the Flame's `zoom`.
// 7. The extras' modules come as they are; their cables from Music's or a modulator's port.
//
// The picture side's wiring (which picture goes into which, and the modules it needs: the Screen, the Feedbacks, the
// Mixes) is the picture graph's (step 3): `pictureWiring`, none until it lands.

import { effectOfType, slotsAsEffects } from './effect-modules';
import { macroAt, mapped } from './macro-map';
import { endOf, type RouteValue } from './curves';
import { ASPECTS, BAND_SIGNALS, COLUMN_SCALES, isRoute, LEVEL_ASPECTS, MOMENT_ASPECTS, PART_ASPECTS, routeKey, type Aspect } from './routes';
import { shapeOf } from './evaluate';
import { DEFAULT_LIMITS, type Budgeted } from './budget';
import { BUSY } from './engine/sidechain';
import { slotKnobOf, slotOf } from './engine/effect-slots';
import { ASPECT_KNOB, cratarrPatch, modulesOf, type Cable, type Macro, type Module, type ModuleType, type Patch } from './patch';
import { isDevice, RANDOMIZE_SCOPES, READINGS, SHOW_PARTS, type CableV2, type Device, type PatchV2, type PortRef } from './ports';
import { extraType } from './registry';
import { modulatorOf } from './modulators/modulators';
import { makeDevice } from './device-make';
import { RACK_NAMES } from './device-names';
import { upgraded } from './upgrade';
import { lookMacros } from './rack-face';
import { v2TypeRecord } from './v2-types';
import { EFFECTS } from './effects';
import { effectType } from './effect-modules';

/** A cable as the conversion carries it until the end: how it reads its signal, its speed and its mode still on it; they
 *  leave the wire at the end (`toWire`). */
type Wire = Omit<CableV2, 'from'> & { from: PortRef & { reference?: (typeof READINGS)[number]; speed?: 'fast' | 'slow' }; mode: 'level' | 'swing' };
import { RANGES, type VisualSettings } from './settings.svelte';

const BANDS = 6;
/** The ADSR on Music's `newtrack`: the Track's pulse (all of it as a track starts, gone over about a beat). */
const NEWTRACK = 'newtrack';

/** The settings that leave the frame for the module that does them (6). */
const MOVED: Record<string, [string, string]> = { intensity: ['envelope', 'intensity'], flash: ['accent', 'accent'], speed: ['clock', 'speed'] };

/** Where a route's signal comes from (2–4). */
function routeFrom(c: Cable): Pick<Wire, 'from' | 'depth'> {
  const from = c.from as { signal: string; band?: number };
  if (from.signal === 'loudness' && from.band === undefined) return { from: { module: 'envelope', port: 'level' }, depth: null };
  if (from.signal === 'track') return { from: { module: NEWTRACK, port: 'out' } };
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

/** The picture side's cables for a patch (step 3); none until the picture graph is in. */
export type Wiring = { modules: Module[]; cables: CableV2[] };
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
      // The flow Bend reads the picture along (0057 §4h: the fluid's read-back), normalled to the Fluid's.
      { as: 'field', label: 'Flow field', module: 'bend', port: 'field' },
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

/** Where a device's exposed ports lead, by `device.port`: an input to one inner port or several (a fan-out, as Bloom's
 *  picture), an output from one. Inputs and outputs apart, as one name may be both. */
type Behind = { in: Map<string, PortRef[]>; out: Map<string, PortRef[]> };

/** `p` flattened, and where its devices' exposed ports lead, through nested ones (fix). */
/** Knob values into a patch by flattened module id (`show/curve`: the Show device's Curve), devices opened; a knob
 *  pinned at its level (`<module>.<knob>`, a band's six by theirs) left as it is, whatever moves it (a step's Next, a
 *  new track's look, the dice). The one way the visuals' own changes are written. */
export function applyKnobs(p: PatchV2, values: Record<string, Record<string, number>>) {
  for (const [id, knobs] of Object.entries(values)) {
    const path = id.split('/');
    let level: PatchV2 | null = p;
    for (const step of path.slice(0, -1)) {
      const d: Module | undefined = level.modules.find((m) => m.id === step);
      level = d && isDevice(d) ? d.patch : null;
      if (!level) break;
    }
    const m = level?.modules.find((x) => x.id === path[path.length - 1]);
    if (!level || !m) continue;
    for (const [k, v] of Object.entries(knobs)) if (!level.pins.includes(`${m.id}.${k}`) && !level.pins.includes(`${m.id}.${k.replace(/\.\d+$/, '')}`)) m.knobs[k] = v;
  }
}

/** A knob of a flattened patch's module as set: its value, its macros' targets on it (along its taper: an exponential
 *  knob's geometrically), as the engine sets its base; undefined where the module has no value for it. */
export function asSet(p: PatchV2, m: Module | undefined, knob: string, types?: Record<string, ModuleType>): number | undefined {
  if (!m) return undefined;
  let v = m.knobs[knob];
  const exp = types?.[m.type]?.knobs.find((k) => k.id === knob)?.taper === 'exp';
  for (const x of p.macros) for (const t of x.targets) if (t.module === m.id && t.knob === knob) v = mapped(t, macroAt(macroAsSet(p, x)), exp);
  return typeof v === 'number' && Number.isFinite(v) ? v : undefined;
}

/** The macro mapping a flattened module's knob (0077: one a knob) or a device's macro (its node's `value`): its name,
 *  its range, and the value as it sets it. */
export function mappingOf(p: PatchV2, id: string, knob: string, types?: Record<string, ModuleType>): { label: string; from: number; to: number; value: number } | undefined {
  for (const x of p.macros)
    for (const t of x.targets)
      if (t.module === id && t.knob === knob) {
        const inner = knob === 'value' ? p.macros.find((y) => y.id === id) : undefined;
        const value = inner ? macroAsSet(p, inner) : asSet(p, p.modules.find((y) => y.id === id), knob, types);
        return value === undefined ? undefined : { label: x.label, from: t.from, to: t.to, value };
      }
  return undefined;
}

/** A flattened patch's macro's value as set: its own, or as the macro one level up that maps it sets it. */
export function macroAsSet(p: PatchV2, x: Macro, seen: Set<string> = new Set([x.id])): number {
  for (const y of p.macros)
    for (const t of y.targets)
      if (t.module === x.id && t.knob === 'value' && !seen.has(y.id)) {
        seen.add(y.id);
        return mapped(t, macroAt(macroAsSet(p, y, seen)));
      }
  return x.value;
}

/** Macro values into a patch by their node's id as the engine runs it (the rack's by its id, a device's
 *  `<device>/macro:<id>`); one pinned at its level (`macro:<id>`) left as it is. The one way a roll's macros are written. */
export function applyMacros(p: PatchV2, values: Record<string, number>) {
  for (const [key, v] of Object.entries(values)) {
    const cut = key.lastIndexOf('/macro:');
    const path = cut < 0 ? [] : key.slice(0, cut).split('/');
    const id = cut < 0 ? key : key.slice(cut + '/macro:'.length);
    let list: { id: string; value: number }[] = p.macros;
    let pins = p.pins;
    let level: PatchV2 | null = p;
    for (const step of path) {
      const d: Module | undefined = level?.modules.find((m) => m.id === step);
      level = d && isDevice(d) ? d.patch : null;
      if (!level || !d || !isDevice(d)) {
        list = [];
        break;
      }
      list = d.macros;
      pins = d.patch.pins;
    }
    const x = list.find((y) => y.id === id);
    if (x && !pins.includes(`macro:${id}`)) x.value = Math.min(1, Math.max(0, v));
  }
}

/** A device's pin as the flattened patch names it: a module's, a cable's or a knob's under the device's path (a
 *  category's and a scope's hold at their own level only). */
const pinAt = (device: string, pin: string): string[] =>
  pin.startsWith('module:') ? [`module:${device}/${pin.slice(7)}`] : pin.startsWith('cable:') ? [`cable:${device}/${pin.slice(6)}`] : /^[^:]+\.[^:]+$/.test(pin) ? [`${device}/${pin}`] : [];

/** A macro's flattened id inside a device (`<device>/macro:<id>`: apart from a module of the same name). */
export const macroNode = (device: string, id: string) => `${device}/macro:${id}`;
/** A level's macro as the flattened patch has it (0077: a macro reaches one level down): a target on a device's macro
 *  (`<device>`, `macro:<id>`) is that macro's node's value; every other, a knob of the level's own modules. */
const ownMacro = (x: Macro): Macro => ({ ...x, targets: x.targets.map((t) => (t.knob.startsWith('macro:') ? { ...t, module: macroNode(t.module, t.knob.slice('macro:'.length)), knob: 'value' } : t)) });

function opened(p: PatchV2): { patch: PatchV2; behind: Behind } {
  const modules: Module[] = [];
  const cables: CableV2[] = [];
  const pins = [...p.pins];
  const macros: Macro[] = p.macros.map(ownMacro);
  const behind: Behind = { in: new Map(), out: new Map() };
  for (const m of p.modules) {
    if (!isDevice(m)) {
      modules.push(m);
      continue;
    }
    const inner = opened(m.patch);
    const at = (id: string) => `${m.id}/${id}`;
    modules.push(...inner.patch.modules.map((x) => ({ ...x, id: at(x.id) })));
    pins.push(...inner.patch.pins.flatMap((x) => pinAt(m.id, x)));
    // Its macros, the macros of the level inside it, under its path: its own (its face) as macros of the flattened patch,
    // each a node of its own; a cable into one (`<device>.macro:<id>`) reaches that node's value.
    const under = (x: Macro): Macro => ({ ...x, id: at(x.id), targets: x.targets.map((t) => ({ ...t, module: at(t.module) })) });
    macros.push(...inner.patch.macros.map(under));
    for (const x of m.macros) {
      macros.push(under(ownMacro({ id: `macro:${x.id}`, on: m.id, label: x.label, value: x.value, targets: x.targets })));
      behind.in.set(`${m.id}.macro:${x.id}`, [{ module: macroNode(m.id, x.id), port: 'value' }]);
    }
    cables.push(...inner.patch.cables.map((c) => ({ ...c, id: at(c.id), from: { ...c.from, module: at(c.from.module) }, to: { ...c.to, module: at(c.to.module) } })));
    for (const [dir, list] of [['in', m.inputs], ['out', m.outputs]] as const)
      for (const e of list) {
        const key = `${m.id}.${e.as}`;
        const map = behind[dir];
        for (const r of inner.behind[dir].get(`${e.module}.${e.port}`) ?? [e]) map.set(key, [...(map.get(key) ?? []), { module: at(r.module), port: r.port, ...(e.channel !== undefined ? { channel: e.channel } : {}) }]);
      }
  }
  const lead = <T extends PortRef>(r: T, dir: 'in' | 'out'): T[] => (behind[dir].get(`${r.module}.${r.port}`) ?? [r]).map((b) => (b === r ? r : { ...r, module: b.module, port: b.port }));
  for (const c of p.cables) {
    const from = lead(c.from, 'out')[0];
    for (const [i, t] of lead(c.to, 'in').entries()) cables.push({ ...c, ...(i ? { id: `${c.id}~${i}` } : {}), from, to: t });
  }
  return { patch: { ...p, modules, cables, pins, macros }, behind };
}

/** The bands' hits through an ADSR (mapping of today's Linger): `adsr` for the hits, `adsr-drums` for the
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
 *  the shape): by column knob, the setting that scales it. The fluid's stir, flow and swirl are scaled where
 *  it stirs, not here. */
const SCALE_OF = new Map(
  (Object.entries(COLUMN_SCALES) as [string, string][]).filter(([a]) => a !== 'stir' && a !== 'flow' && a !== 'swirl').map(([a, key]) => [ASPECT_KNOB[a as keyof typeof ASPECT_KNOB].join('.'), key]),
);

/** Each column knob's aspect (today's chain), by `module.knob`. */
const ASPECT_AT = new Map((Object.entries(ASPECT_KNOB) as [Aspect, [string, string]][]).map(([a, [m, k]]) => [`${m}.${k}`, a]));
const LEVEL_KNOBS = new Set([...ASPECT_AT].filter(([, a]) => LEVEL_ASPECTS.includes(a)).map(([k]) => k));

/** Cables only add (0057 §7e items 1, 3, 4; the shapes, proved frame by frame over the recorded tracks):
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
  const banded = (c: Wire) => c.from.channel !== undefined || (c.from.module === 'music' && (BAND_SIGNALS as readonly string[]).includes(c.from.port)) || (typeOf.get(c.from.module) === 'adsr' && c.from.module !== NEWTRACK);
  for (const c of cables) {
    const key = `${c.to.module}.${c.to.port}`;
    // An effect module's knob is its slot's column (0057 §3).
    const slot = slotOf(c.to.module, typeOf.get(c.to.module) ?? '');
    const knob = slot && slotKnobOf(typeOf.get(c.to.module)!, c.to.port);
    const aspect = ASPECT_AT.get(slot && knob ? `${slot}.${knob}` : key);
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

/** The sidechain in sight (0057 §7e; the wiring, proved against today's levers): a Meter on the picture, a
 *  Ducker a kind keyed by it (its threshold the limit as learned and frozen, × Wild's factor, × the music's activity
 *  through a VCA from the Loudness's level), each into the gain of what that kind of busyness moves; the Screen's flash
 *  guard into the Accent's gain. */
/** A Ducker's threshold over the limit it holds its kind to (toV2's margin: Wild at ½ gives exactly the limit × 1.5). */
const THRESHOLD_OVER_LIMIT = 1.5;
const SIDECHAIN: readonly { kind: string; meter: Budgeted; into: readonly [string, string] | null }[] = [
  { kind: 'motion', meter: 'motion', into: ['clock', 'speed'] },
  { kind: 'shape', meter: 'shapeChurn', into: ['flame', 'evolution'] },
  { kind: 'colour', meter: 'colourChurn', into: ['flame', 'hue'] },
  { kind: 'zoom', meter: 'zoomPumping', into: ['flame', 'zoom'] },
  // The moments' Gates (null): their gaps stretched.
  { kind: 'looks', meter: 'looksPerMinute', into: null },
];
function sidechained(modules: Module[], cables: Wire[], wild: number, limits: Partial<Record<Budgeted, number>>, holds = false): { modules: Module[]; cables: Wire[]; macro?: Macro } {
  const one = (id: string, from: Wire['from'], to: Wire['to'], value = 1): Wire => ({ id, from, to, value, mode: 'swing', depth: null });
  const out = { modules: [...modules, { id: 'meter', type: 'meter', knobs: {} }], cables: [...cables] };
  // The Screen is the picture wiring's (`screen`); its guard cabled from here.
  out.cables.push(one('guard', { module: 'screen', port: 'guard' }, { module: 'accent', port: 'accent:gain' }));
  const wildFactor = 2 ** (2 * (wild - 0.5));
  // Wild, a macro on the five thresholds (the owner): along the threshold's exponential taper from half to twice
  // (× 1.5) of the limit, so at today's Wild it is exactly today's factor 2^(2(wild − ½)).
  const macro: Macro = { id: 'wild', on: 'rack', label: 'Wild', value: wild, targets: [] };
  for (const { kind, meter, into } of SIDECHAIN) {
    const id = `duck-${kind}`;
    const limit = limits[meter] ?? DEFAULT_LIMITS[meter];
    macro.targets.push({ module: id, knob: 'threshold', from: limit * THRESHOLD_OVER_LIMIT * 0.5, to: limit * THRESHOLD_OVER_LIMIT * 2 });
    out.modules.push({ id, type: 'ducker', knobs: { threshold: limit * wildFactor * THRESHOLD_OVER_LIMIT, depth: Math.abs(20 * Math.log10(BUSY[meter].most)), attack: 1 / 0.7, hold: 0, release: 10 / 3, hysteresis: 0.8 } });
    out.cables.push(one(`${id}-key`, { module: 'meter', port: kind }, { module: id, port: 'key' }));
    out.modules.push({ id: `${id}-activity`, type: 'vca', knobs: { gain: 1 / 3 } });
    out.cables.push(one(`${id}-loudness`, { module: 'envelope', port: 'level' }, { module: `${id}-activity`, port: 'cv' }, 2 / 3));
    out.cables.push(one(`${id}-activity-out`, { module: `${id}-activity`, port: 'out' }, { module: id, port: 'threshold:gain' }));
    const targets: (readonly [string, string])[] = into ? [into] : modules.filter((m) => m.type === 'gate' && m.id.startsWith('gate-')).map((m) => [m.id, 'gap'] as const);
    // The motion's also slows the zooms (the levers' pace, on cables now: 0057 audit row 2).
    if (holds && kind === 'motion') targets.push(...(['tunnel', 'dive'] as const).filter((m) => modules.some((x) => x.id === m)).map((m) => [m, m] as const));
    for (const [module, knob] of targets) out.cables.push(one(`${id}-${module}`, { module: id, port: 'out' }, { module, port: `${knob}:gain` }));
  }
  // Each effect held back by the flash guard and by the Duckers of what it adds (its report: motion, shape churn, colour
  // churn, zoom pumping), as the levers did out of sight: their product into its Amount's gain, through VCAs (a gain's
  // cables add, a VCA multiplies: in × cv at gain 0).
  const KIND: Record<string, string> = { motion: 'motion', shapeChurn: 'shape', colourChurn: 'colour', zoomPumping: 'zoom' };
  for (const m of holds ? modules : []) {
    const e = EFFECTS.find((x) => effectType(x) === m.type);
    if (!e) continue;
    const r = e.report(1, Object.fromEntries(e.knobs.map((_, i) => [`k${i + 1}`, 0.5])) as never) as Record<string, number>;
    const factors: Wire['from'][] = [{ module: 'screen', port: 'guard' }, ...Object.keys(KIND).filter((k) => r[k]).map((k) => ({ module: `duck-${KIND[k]}`, port: 'out' }))];
    let at = factors[0];
    factors.slice(1).forEach((f, i) => {
      const vca = `${m.id}-held-${i + 1}`;
      out.modules.push({ id: vca, type: 'vca', knobs: { gain: 0 } });
      out.cables.push(one(`${vca}-in`, at, { module: vca, port: 'in' }), one(`${vca}-cv`, f, { module: vca, port: 'cv' }));
      at = { module: vca, port: 'out' };
    });
    out.cables.push(one(`${m.id}-held`, at, { module: m.id, port: 'amount:gain' }));
  }
  return { ...inSeries(out), macro };
}

/** A gain input with several cables (a column's own VCAs and a sidechain's hold): its cables multiplied through VCAs
 *  in series (in × cv at gain 0), as a gain's cables would otherwise add. */
function inSeries(p: { modules: Module[]; cables: Wire[] }): { modules: Module[]; cables: Wire[] } {
  const into = new Map<string, Wire[]>();
  for (const c of p.cables) if (c.to.port.endsWith(':gain')) (into.get(`${c.to.module}|${c.to.port}`) ?? into.set(`${c.to.module}|${c.to.port}`, []).get(`${c.to.module}|${c.to.port}`)!).push(c);
  const modules = [...p.modules];
  const moved = new Map<Wire, Wire>();
  const added: Wire[] = [];
  for (const [key, cs] of into) {
    if (cs.length < 2) continue;
    const [module, port] = key.split('|');
    const base = `${module}-${port.slice(0, -':gain'.length)}-times`;
    let at: Wire['from'] | null = null;
    cs.forEach((c, i) => {
      if (i === cs.length - 1) {
        moved.set(c, { ...c, to: { module: at ? `${base}-${i}` : module, port: at ? 'cv' : port } });
        if (at) added.push({ id: `${base}-${i}-out`, from: { module: `${base}-${i}`, port: 'out' }, to: { module, port }, value: 1, mode: 'swing', depth: null });
        return;
      }
      const id = `${base}-${i + 1}`;
      if (i === 0) {
        modules.push({ id, type: 'vca', knobs: { gain: 0 } });
        moved.set(c, { ...c, to: { module: id, port: 'in' } });
      } else {
        modules.push({ id, type: 'vca', knobs: { gain: 0 } });
        moved.set(c, { ...c, to: { module: `${base}-${i}`, port: 'cv' } });
        added.push({ id: `${base}-${i}-out`, from: { module: `${base}-${i}`, port: 'out' }, to: { module: id, port: 'in' }, value: 1, mode: 'swing', depth: null });
      }
      at = { module: id, port: 'out' };
    });
  }
  return { modules, cables: [...p.cables.map((c) => moved.get(c) ?? c), ...added] };
}

/** The cables as wires (0057 §7e item 10): how each signal is read goes to its Music row (`reference.<signal>`: 'absolute'
 *  wherever a route reads the row, as routes do, else what its cables read; a cable reading otherwise keeps its row's,
 *  rare), and an ADSR's gate's to that ADSR; a slow reading through an Envelope Follower at the slow times (one a
 *  signal and band, `slow-<signal>[-<band>]`); the mode gone (cables only add, the levels already through VCAs). */
function toWire(given: Module[], all: Wire[]): { modules: Module[]; cables: CableV2[] } {
  const modules = [...given];
  const followers = new Map<string, Wire>();
  const wires = all.map((w): Wire => {
    if (w.from.speed !== 'slow' || w.from.module !== 'music') return w;
    const { speed: _speed, ...from } = w.from;
    // An ADSR's gate: its release is its own slowness.
    if (modules.find((m) => m.id === w.to.module)?.type === 'adsr') return { ...w, from };
    const id = ['slow', from.port, from.channel].filter((x) => x !== undefined).join('-');
    if (!followers.has(id)) {
      modules.push({ id, type: 'envelope', knobs: { attack: 0.4, release: 1.5, intensity: 1 } });
      followers.set(id, { id: `${id}-in`, from, to: { module: id, port: 'in' }, value: 1, mode: 'swing', depth: null });
    }
    // The band now the cable's end's (the follower is one channel).
    const to = from.channel !== undefined && w.to.channel === undefined ? { ...w.to, channel: from.channel } : w.to;
    return { ...w, from: { module: id, port: 'level' }, to };
  });
  wires.push(...followers.values());
  const music = modules.find((m) => m.id === 'music');
  const reading = new Map<string, (typeof READINGS)[number]>();
  for (const w of wires) {
    if (w.from.module !== 'music' || modules.find((m) => m.id === w.to.module)?.type === 'adsr') continue;
    const r = w.from.reference ?? 'track';
    const was = reading.get(w.from.port);
    if (!was || r === 'absolute') reading.set(w.from.port, r);
  }
  if (music) for (const [signal, r] of reading) if (r !== 'track') music.knobs[`reference.${signal}`] = READINGS.indexOf(r);
  const cables = wires.map(({ mode: _mode, ...w }) => {
    const { reference, speed: _speed, ...from } = w.from;
    const adsr = modules.find((m) => m.id === w.to.module && m.type === 'adsr');
    if (adsr && from.module === 'music') adsr.knobs.reference = READINGS.indexOf(reference ?? 'track');
    return { ...w, from };
  });
  return { modules, cables };
}

/** Today's settings (with their extras) as a v2 patch.
 *  - `addOnly` (default true): cables only add (0057 §7e); false gives the patch before that step, for the proof.
 *  - `sidechain` (default true): the sidechain in sight; false leaves it out, for the proof.
 *  - `limits`: the limits as learned (the app's `limitsNow`), frozen into the Duckers; else the defaults. */
export function toV2(s: VisualSettings, opts: { addOnly?: boolean; sidechain?: boolean; limits?: Partial<Record<Budgeted, number>>; devices?: boolean; follow?: boolean; holds?: boolean } = {}): PatchV2 {
  const p = cratarrPatch(s);
  // Drift is Random now (its smooth end: Shape 0), as VCV, Vital and Bitwig name it (the owner).
  const modules: Module[] = [...modulesOf(p).values()].map((m) => (m.type === 'drift' ? { ...m, type: 'random', knobs: { ...m.knobs, shape: 0 } } : { ...m, knobs: { ...m.knobs } }));
  const frame = modules.find((m) => m.id === 'frame');
  const engine = new Map<string, Module>([
    ['music', { id: 'music', type: 'music', knobs: {} }],
    ['envelope', { id: 'envelope', type: 'envelope', knobs: { attack: 0.2, release: 0.2 } }],
    ['clock', { id: 'clock', type: 'clock', knobs: { beats: 32 } }],
    ['accent', { id: 'accent', type: 'accent', knobs: { course: 0.3 } }],
  ]);
  for (const [setting, [module, knob]] of Object.entries(MOVED)) {
    engine.get(module)!.knobs[knob] = (s as unknown as Record<string, number>)[setting];
    if (frame) delete frame.knobs[setting];
  }
  // The Whole frame goes (0057): Intensity, Flash and Speed rack macros on the knobs they moved to, Wild one on the
  // Duckers' thresholds (sidechained), Effects one on the effects' Amounts (below); the device's own settings (quality,
  // frame rate, brightness) the device's; the walks Randoms now (§7e item 5), their columns gone with it.
  const rackMacros: Macro[] = (Object.entries(MOVED) as [string, [string, string]][]).map(([setting, [module, knob]]) => {
    const [lo, hi] = RANGES[setting as keyof typeof RANGES];
    const v = (s as unknown as Record<string, number>)[setting];
    return { id: setting, on: 'rack', label: RACK_NAMES[setting] ?? setting[0].toUpperCase() + setting.slice(1), value: Math.min(1, Math.max(0, (v - lo) / (hi - lo))), targets: [{ module, knob, from: lo, to: hi }] };
  });
  if (frame) modules.splice(modules.indexOf(frame), 1);
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
  // A column's scale goes into its cables; its knob is then the column's resting value, the cables adding to it
  // (VCV's rule; 0, as today rests).
  const scaleOf = (to: Cable['to']) => {
    const key = SCALE_OF.get(`${to.module}.${to.knob}`);
    return key ? ((s as unknown as Record<string, number>)[key] ?? 1) : 1;
  };
  for (const key of SCALE_OF.keys()) {
    const [mid, kid] = key.split('.');
    // Show's are inside its device (below); its size is the Flame's zoom.
    const m = mid === 'show' ? undefined : modules.find((x) => x.id === mid);
    if (m) m.knobs[kid] = 0;
  }
  const showAsDevice = modules.find((x) => x.id === 'show') as Device | undefined;
  for (const part of showAsDevice?.patch.modules ?? []) for (const k of Object.keys(part.knobs)) if (SCALE_OF.has(`show.${k}`)) part.knobs[k] = 0;
  for (const c of p.cables) {
    if (c.to.module === 'frame' && c.to.knob !== 'effects') continue;
    // The Effects column's routes: each effect's Amount's too, in series with its own (below).
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
  // The Track's pulse: an ADSR struck by a new track, where something reads it.
  const newtrack: Module[] = [];
  if (cables.some((c) => c.from.module === NEWTRACK)) {
    const knobs: Record<string, number> = { retrigger: 0, amount: 1, reference: READINGS.indexOf('absolute') };
    for (let b = 0; b < BANDS; b++) Object.assign(knobs, { [`attack.${b}`]: 0, [`decay.${b}`]: 0, [`sustain.${b}`]: 0, [`release.${b}`]: 0.25 });
    newtrack.push({ id: NEWTRACK, type: 'adsr', knobs });
    cables.push({ id: `${NEWTRACK}-gate`, from: { module: 'music', port: 'newtrack', reference: 'absolute' }, to: { module: NEWTRACK, port: 'gate' }, value: 1, mode: 'swing', depth: null });
  }
  // Pausing dims in sight (0057 §7e item 6): 1 − playing, followed over half a second, lowers Tone map's
  // Level gain by up to 0.45 (0.55 paused, 1 playing), starting at 1 as the visuals open.
  const pause: Module[] = [
    { id: 'pause-invert', type: 'vca', knobs: { gain: 1 } },
    { id: 'pause-follow', type: 'envelope', knobs: { attack: 0.5, release: 0.5, intensity: 1 } },
    { id: 'pause-dim', type: 'vca', knobs: { gain: 1 } },
  ];
  cables.push(
    { id: 'pause-playing', from: { module: 'music', port: 'playing' }, to: { module: 'pause-invert', port: 'cv' }, value: -1, mode: 'swing', depth: null },
    { id: 'pause-invert-out', from: { module: 'pause-invert', port: 'out' }, to: { module: 'pause-follow', port: 'in' }, value: 1, mode: 'swing', depth: null },
    { id: 'pause-follow-out', from: { module: 'pause-follow', port: 'out' }, to: { module: 'pause-dim', port: 'cv' }, value: -0.45, mode: 'swing', depth: null },
    { id: 'pause-dim-out', from: { module: 'pause-dim', port: 'out' }, to: { module: 'show', port: 'level:gain' }, value: 1, mode: 'swing', depth: null },
  );
  // A new track's own look (0057 §7f, "same randomize"): the Dice on Music's New track, seeded by the Track seed,
  // wherever the Track row's route into Look was on (as it is by default).
  const dice: Module[] = [];
  if (endOf(s.routes[routeKey('track', null, 'look')] ?? 0) !== 0) {
    // How much of the look a new track rolls anew: the route's amount (−100 %: all of it).
    // The face it rolls (0077: the dice play the face): every macro of every level.
    dice.push({ id: 'dice', type: 'dice', knobs: { scope: RANDOMIZE_SCOPES.indexOf('macros'), reach: Math.min(1, Math.abs(endOf(s.routes[routeKey('track', null, 'look')] ?? 0))) } });
    cables.push(
      { id: 'dice-trigger', from: { module: 'music', port: 'newtrack', reference: 'absolute' }, to: { module: 'dice', port: 'trigger' }, value: 1, mode: 'swing', depth: null },
      { id: 'dice-seed', from: { module: 'music', port: 'seed', reference: 'absolute' }, to: { module: 'dice', port: 'seed' }, value: 1, mode: 'swing', depth: null },
    );
  }
  // One module per effect (0057 §3): each slot as the module of the effect it draws, its cables and pins with it.
  const effects = slotsAsEffects(modules, cables, p.pins, p.macros);
  // Effects, the most of the effects, a rack macro on each one's Amount (0 to as it is set); its routes into each
  // Amount beside the Amount's own, so their VCAs run in series (the Effects column's share times the Amount's).
  const fxModules = effects.modules.filter((m) => effectOfType(m.type));
  const intoEffects = effects.cables.filter((c) => c.to.module === 'frame');
  effects.cables = effects.cables.filter((c) => c.to.module !== 'frame');
  for (const fx of fxModules) for (const c of intoEffects) effects.cables.push({ ...c, id: `${c.id}-${fx.id}`, to: { module: fx.id, port: 'amount' } });
  rackMacros.push({ id: 'effects', on: 'rack', label: 'Effects', value: Math.min(1, Math.max(0, s.effects)), targets: fxModules.map((fx) => ({ module: fx.id, knob: 'amount', from: 0, to: fx.knobs.amount ?? 1 })) });
  const all = [...engine.values(), ...newtrack, ...[...adsrs.values()].map((a) => a.module), ...effects.modules, ...pause, ...dice];
  const adding = opts.addOnly === false ? { modules: all, cables: effects.cables } : cablesAdd(all, effects.cables);
  const added = opts.sidechain === false ? adding : sidechained(adding.modules, adding.cables, s.wild, opts.limits ?? {}, opts.holds !== false);
  const wiring = pictureWiring(p);
  const wildMacro = (added as { macro?: Macro }).macro;
  // The rack's face (0077): the look's macros over the Flame, then Effects, then the behaviour (Intensity, Accent, Speed,
  // Wild); a kept patch's own macros after them.
  const flameModule = added.modules.find((m) => m.type === 'flame');
  const byId = (id: string) => rackMacros.find((x) => x.id === id);
  const face = [...(flameModule ? lookMacros(flameModule) : []), ...(['effects', 'intensity', 'flash', 'speed'].map(byId).filter((x): x is Macro => !!x)), ...(wildMacro ? [wildMacro] : [])];
  const macros: Macro[] = [...face, ...rackMacros.filter((x) => !face.includes(x)), ...effects.macros];
  const wired = toWire(added.modules, added.cables);
  const rack: PatchV2 = { v: 2, modules: [...wired.modules, ...wiring.modules], cables: [...wired.cables, ...wiring.cables], macros, pins: effects.pins };
  // The loudness's VCAs in sight (`follow`, on unless asked not to: the engine has no normal).
  const follows = opts.follow !== false ? followed(rack) : rack;
  return opts.devices === false ? follows : packed(follows);
}

/** The common patterns as devices (0057 §7f: "common patterns that replace removed special cases ship as ready-made
 *  devices"): the sidechain's Duckers and their activity VCAs as Calm, the pause's dimming as Pause Dim, the hits'
 *  envelopes as Hit Lights. Each opens to its modules and can be unmade. */
/** The loudness scaling every cable, in sight (0057 §7f): where a cable's depth was the Envelope Follower's by its
 *  normal, it goes through a VCA keyed by the Follower instead (gain 0, its gain input the Follower's out: in × out).
 *  Straight cables from one source share one VCA after it, their amounts on its cables out; a curved one gets its own
 *  after its curve (the owner: curved cables kept exact). The Follower is the Loudness: the first with no input cabled.
 *  Then no cable is scaled by a normal: every depth is cabled or none. */
function followed(p: PatchV2): PatchV2 {
  const follower = p.modules.find((m) => m.type === 'envelope' && !p.cables.some((c) => c.to.module === m.id && c.to.port === 'in'));
  if (!follower || !p.cables.some((c) => c.depth === undefined && !c.id.startsWith('picture:'))) return p;
  // Ahead of every other module: the engine steps its VCAs in the rack's order, and these feed the rest this frame.
  const follows: Module[] = [];
  const cables: CableV2[] = [];
  const shared = new Map<string, string>();
  let n = 0;
  const vca = (): string => {
    const id = `follow-${++n}`;
    follows.push({ id, type: 'vca', knobs: { gain: 0 } });
    cables.push({ id: `${id}-key`, from: { module: follower.id, port: 'out' }, to: { module: id, port: 'cv' }, value: 1, depth: null });
    return id;
  };
  for (const c of p.cables) {
    // A picture's cable (picture-wiring.ts) is no value scaled.
    // Nor is an ADSR's gate (its strikes are Music's) or a modulator's input: the normal never scaled them.
    const toType = p.modules.find((m) => m.id === c.to.module)?.type ?? '';
    const mod = modulatorOf(toType);
    if (c.depth !== undefined || c.id.startsWith('picture:') || (c.to.port === 'gate' && toType === 'adsr') || (mod && [...(mod.inputs ?? []), ...(mod.triggers ?? [])].includes(c.to.port))) {
      cables.push(c);
      continue;
    }
    const channel = c.from.channel !== undefined ? { channel: c.from.channel } : {};
    if (typeof c.value === 'number') {
      const key = `${c.from.module}.${c.from.port}.${c.from.channel ?? ''}`;
      let id = shared.get(key);
      if (!id) {
        id = vca();
        shared.set(key, id);
        cables.push({ id: `${id}-in`, from: c.from, to: { module: id, port: 'in' }, value: 1, depth: null });
      }
      cables.push({ ...c, from: { module: id, port: 'out', ...channel }, depth: null });
    } else {
      const id = vca();
      cables.push({ id: `${id}-in`, from: c.from, to: { module: id, port: 'in' }, value: c.value, depth: null });
      cables.push({ ...c, from: { module: id, port: 'out', ...channel }, value: 1, depth: null });
    }
  }
  return { ...p, modules: [...follows, ...p.modules], cables };
}

function packed(p: PatchV2): PatchV2 {
  const ids = (f: (m: Module) => boolean) => p.modules.filter(f).map((m) => m.id);
  makeDevice(p, ids((m) => m.id.startsWith('duck-')), { id: 'calm', label: 'Calm' });
  makeDevice(p, ids((m) => m.id.startsWith('pause-')), { id: 'pause', label: 'Pause Dim' });
  makeDevice(p, ids((m) => m.id.startsWith('follow-')), { id: 'loudness-follow', label: 'Loudness Follow' });
  makeDevice(p, ids((m) => m.type === 'adsr' && m.id !== NEWTRACK), { id: 'hit-lights', label: 'Hit Lights' });
  return upgraded(p, v2TypeRecord());
}
