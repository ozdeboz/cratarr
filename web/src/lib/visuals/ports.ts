// Typed ports (decision 0057, step 1): the interfaces the one engine (claude-5f), the picture graph (claude-632) and the
// views build on. Three kinds of signal: a picture (a texture each frame), a value (a number each frame), a trigger
// (events with their strength). A module type's ports come from what it declares: its knobs are value inputs, a
// modulator's `inputs` are value inputs, a picture module takes and gives a picture, a modulator gives a value. The
// modules that hold today's hidden behaviours are declared here, so every one of them shows.
import type { RouteValue } from './curves';
import { BUSY, type Knob, type Macro, type Module, type ModuleType, type Place } from './patch';
import { BAND_SIGNALS, TRACK_SIGNALS } from './routes';

/** `light`: the flame's accumulated light (its RGBA and its bands' shares) before Show turns it into a picture; the
 *  flame's own loop works on it (claude-632, step 3). Only Show converts light into a picture. */
/** `audio`: the sound itself, its recent samples (Music gives it; the Waves source draws it). */
export type PortType = 'picture' | 'light' | 'audio' | 'value' | 'trigger';

/** A port: its id on its module, its name, its type; `channels`: a band's six at once (VCV's polyphonic cable; a
 *  cable may take one of them); `normal`: what it is wired to unless a cable is plugged in (Eurorack's normalled
 *  connection: shown faint, broken by a cable). */
export type Port = { id: string; label: string; type: PortType; channels?: number; normal?: PortRef };

/** One end of a cable: a module's port, a channel of it where it has several. */
export type PortRef = { module: string; port: string; channel?: number };

export type Ports = { in: Port[]; out: Port[] };

/** The signals that are events: a beat, a band's hits. Into a value port they come as a pulse. */
export const TRIGGER_SIGNALS: ReadonlySet<string> = new Set(['beat', 'downbeat', 'section', 'hits', 'percussiveHits']);

/** Whether a cable may go from a port of `from` type into one of `to` type: the same, or a trigger into a value. */
export const fits = (from: PortType, to: PortType) => from === to || (from === 'trigger' && to === 'value');

const knobPort = (k: Knob): Port => ({ id: k.id, label: k.label, type: 'value' });
const PICTURE_IN: Port = { id: 'picture', label: 'Picture', type: 'picture' };
const PICTURE_OUT: Port = { id: 'picture', label: 'Picture', type: 'picture' };
const VALUE_OUT: Port = { id: 'out', label: 'Out', type: 'value' };
const TAKES_PICTURE: ReadonlySet<Place> = new Set<Place>(['module', 'loop', 'finish']);

/** A module type's ports, from what it declares (its own `ports` where it has them: the modules below). */
export function portsOf(t: ModuleType & { ports?: Ports }): Ports {
  if (t.ports) return t.ports;
  const knobs = t.knobs.filter((k) => !k.device).map(knobPort);
  const inputs = (t.inputs ?? []).map((id): Port => ({ id, label: id === 'in' ? 'Input' : id[0].toUpperCase() + id.slice(1), type: id === 'trigger' ? 'trigger' : 'value' }));
  if (t.place === 'modulator') return { in: [...inputs, ...knobs], out: [VALUE_OUT] };
  if (t.place === 'frame') return { in: knobs, out: [] };
  return { in: [...(TAKES_PICTURE.has(t.place) ? [PICTURE_IN] : []), ...knobs], out: [PICTURE_OUT] };
}

const knob = (id: string, label: string, min: number, max: number, def: number, kind: Knob['kind'] = 'swing', neutral = kind === 'level' ? min : def): Knob => ({
  id,
  label,
  min,
  max,
  neutral,
  default: def,
  kind,
});
const out = (id: string, label: string, type: PortType = 'value', channels?: number): Port => ({ id, label, type, ...(channels ? { channels } : {}) });
const label = (id: string) => id.replace(/([A-Z])/g, ' $1').replace(/^./, (c) => c.toUpperCase());

type Declared = ModuleType & { ports: Ports };

/** Today's hidden behaviours as modules (0057 §3). Their knobs are what the old settings held; their ports normalled
 *  to where the behaviour reaches today. claude-5f's engine steps them; the views draw them as any other. */
export const MUSIC: Declared = {
  type: 'music',
  label: 'Music',
  place: 'modulator',
  knobs: [],
  ports: {
    in: [],
    out: [
      out('audio', 'Sound', 'audio'),
      ...TRACK_SIGNALS.map((s) => out(s, label(s), TRIGGER_SIGNALS.has(s) ? 'trigger' : 'value')),
      ...BAND_SIGNALS.map((s) => out(s, label(s), TRIGGER_SIGNALS.has(s) ? 'trigger' : 'value', 6)),
    ],
  },
};

export const TRACK: Declared = {
  type: 'track',
  label: 'Track',
  place: 'modulator',
  knobs: [],
  ports: { in: [], out: [out('pulse', 'New track'), out('look', 'Its look', 'trigger')] },
};

/** A module declared by its knobs (each a value input), its other inputs before them, and its outputs. */
function declared(type: string, name: string, place: Place, knobs: Knob[], inputs: Port[], outputs: Port[]): Declared {
  return { type, label: name, place, knobs, ports: { in: [...inputs, ...knobs.map(knobPort)], out: outputs } };
}

export const ENVELOPE = declared(
  'envelope',
  'Loudness',
  'modulator',
  [knob('intensity', 'Intensity', 0, 1, 0.5, 'level', 0), knob('time', 'Time', 0.02, 2, 0.2)],
  [{ id: 'in', label: 'Input', type: 'value', normal: { module: 'music', port: 'loudness' } }],
  // `level`: the loudness as Intensity lets it count (what a loudness cable reads); `out`: that while playing, else 0
  // (every other cable's depth is normalled to it).
  [out('level', 'Level'), VALUE_OUT],
);

export const CLOCK = declared(
  'clock',
  'Clock',
  'modulator',
  [knob('speed', 'Speed', 0, 2, 1, 'level', 0), knob('beats', 'Beats a turn', 4, 128, 32)],
  [{ id: 'tempo', label: 'Tempo', type: 'value', normal: { module: 'music', port: 'tempo' } }],
  [out('phase', 'Turn'), out('rate', 'Rate'), out('beat', 'Beat', 'trigger'), out('bar', 'Bar', 'trigger'), out('phrase', 'Phrase', 'trigger')],
);

export const FLASH = declared(
  'flash',
  'Flash',
  'modulator',
  [knob('flash', 'Flash', 0, 2, 1, 'level', 0), knob('course', 'Course', 0.05, 2, 0.3)],
  [{ id: 'in', label: 'Input', type: 'value' }],
  [VALUE_OUT],
);

export const WALK = declared('walk', 'Walk', 'modulator', [knob('amount', 'Amount', 0, 1, 0, 'level'), knob('rate', 'Rate', 0, 1, 0.1)], [{ id: 'trigger', label: 'Jump', type: 'trigger' }], [VALUE_OUT]);

const BUSY_LABEL: Record<(typeof BUSY)[number], string> = { motion: 'Motion', shape: 'Shape churn', colour: 'Colour churn', zoom: 'Zoom pumping', looks: 'New looks' };

/** A Meter (0057): a detector; the picture it watches in, its busyness out (meters.ts's measures). */
export const METER = declared('meter', 'Meter', 'modulator', [], [PICTURE_IN], BUSY.map((b) => out(b, BUSY_LABEL[b])));

/** A Compressor (0057): a standard sidechain compressor on values (Giannoulis, Massberg, Reiss 2012): `in` compressed
 *  by how far `key` stands over the threshold (a plain knob: no learning in this version); `reduction` the gain reduction (1: none), for ducking anything else. */
export const COMPRESSOR = declared(
  'compressor',
  'Compressor',
  'modulator',
  [
    knob('threshold', 'Threshold', 0, 4, 1),
    knob('ratio', 'Ratio', 1, 20, 4),
    knob('knee', 'Knee', 0, 2, 0.2),
    knob('attack', 'Attack', 0.001, 2, 0.25),
    knob('release', 'Release', 0.01, 10, 2),
    knob('makeup', 'Makeup', 0, 4, 1),
  ],
  [
    { id: 'in', label: 'Input', type: 'value' },
    { id: 'key', label: 'Sidechain', type: 'value' },
  ],
  [VALUE_OUT, out('reduction', 'Reduction')],
);


/** ADSR (0057 §4b, VCV's): an envelope on each gate or trigger; polyphonic, one a channel (Music's hits: one a band). */
export const ADSR = declared(
  'adsr',
  'ADSR',
  'modulator',
  [knob('attack', 'Attack', 0, 2, 0.01), knob('decay', 'Decay', 0, 4, 0.2), knob('sustain', 'Sustain', 0, 1, 0, 'level'), knob('release', 'Release', 0, 8, 0.2), knob('amount', 'Amount', 0, 1, 1, 'level')],
  [{ id: 'gate', label: 'Gate', type: 'trigger', channels: 6 }],
  [out('out', 'Out', 'value', 6)],
);

/** Sequencer (0057 §4b, VCV's SEQ): sixteen steps, each a knob, stepped by its clock, back to the first on reset. */
export const SEQUENCER = declared(
  'sequencer',
  'Sequencer',
  'modulator',
  [knob('length', 'Steps', 1, 16, 8, 'step'), ...Array.from({ length: 16 }, (_, i) => knob(`step${i + 1}`, `Step ${i + 1}`, 0, 1, 0.5))],
  [
    { id: 'clock', label: 'Clock', type: 'trigger', normal: { module: 'clock', port: 'beat' } },
    { id: 'reset', label: 'Reset', type: 'trigger', normal: { module: 'clock', port: 'bar' } },
  ],
  [VALUE_OUT, out('gate', 'Gate', 'trigger')],
);

/** Looper (0057 §4b): a value recorded over its bars and looped in time with the Clock. */
export const LOOPER = declared(
  'looper',
  'Looper',
  'modulator',
  [knob('bars', 'Bars', 1, 16, 4, 'step'), knob('record', 'Record', 0, 1, 0, 'step'), knob('overdub', 'Overdub', 0, 1, 0, 'step')],
  [
    { id: 'in', label: 'Input', type: 'value' },
    { id: 'clock', label: 'Clock', type: 'trigger', normal: { module: 'clock', port: 'bar' } },
  ],
  [VALUE_OUT],
);

/** Show's parts (0057): each of its knobs, the module that takes it (id and type alike), in Show's order. Tone map
 *  turns the flame's light into a picture; the rest are picture modules. */
export const SHOW_PARTS: readonly { knob: string; type: string; label: string }[] = [
  { knob: 'glow', type: 'tonemap', label: 'Tone map' },
  { knob: 'haze', type: 'curve', label: 'Curve' },
  { knob: 'saturation', type: 'saturate', label: 'Saturate' },
  { knob: 'smoothing', type: 'blur', label: 'Blur' },
  { knob: 'size', type: 'scale', label: 'Scale' },
  { knob: 'waves', type: 'bend', label: 'Bend' },
];

/** Show's parts as module types, each with its knob as Show's type has it (`show`: patch.ts cratarrTypes's). */
export function showPartTypes(show: ModuleType): Declared[] {
  return SHOW_PARTS.map(({ knob: id, type, label: name }) => {
    const k = show.knobs.find((x) => x.id === id);
    const knobs = k ? [k] : [];
    const input: Port = type === 'tonemap' ? { id: 'light', label: 'Light', type: 'light' } : PICTURE_IN;
    return declared(type, name, 'module', knobs, [input], [PICTURE_OUT]);
  });
}

export const ENGINE_MODULES: readonly Declared[] = [MUSIC, TRACK, ENVELOPE, CLOCK, FLASH, WALK, METER, COMPRESSOR, ADSR, SEQUENCER, LOOPER];

// ---- The v2 patch (0057 §4, §7): only modules and cables; layers, loops and the finish are views of the wiring ------

/** A cable between typed ports. From a signal port of Music it keeps 0056's reference and speed; its `depth` is the
 *  port that scales it (undefined: normalled to the Envelope; null: unplugged). */
export type CableV2 = {
  id: string;
  from: PortRef & { reference?: 'track' | 'lately' | 'absolute'; speed?: 'fast' | 'slow' };
  to: PortRef;
  value: RouteValue;
  mode: 'level' | 'swing';
  depth?: PortRef | null;
};

export type PatchV2 = { v: 2; modules: Module[]; cables: CableV2[]; macros: Macro[]; pins: string[] };

/** The blends a Mix input takes (0056's, as a step knob per input). */
export const MIX_MODES = ['add', 'screen', 'over', 'multiply', 'window', 'lighten'] as const;
/** How many pictures a Mix takes. */
export const MIX_INPUTS = 4;

/** Mix: up to four pictures, each by its mode and amount, the first under the rest (0057 §4: blend as a module). */
export const MIX = declared(
  'mix',
  'Mix',
  'module',
  Array.from({ length: MIX_INPUTS }, (_, i) => [
    knob(`mode${i + 1}`, `Mode ${i + 1}`, 0, MIX_MODES.length - 1, i === 0 ? 2 : 1, 'step'),
    knob(`amount${i + 1}`, `Amount ${i + 1}`, 0, 1, 1, 'level'),
  ]).flat(),
  Array.from({ length: MIX_INPUTS }, (_, i): Port => ({ id: `in${i + 1}`, label: `Picture ${i + 1}`, type: 'picture' })),
  [PICTURE_OUT],
);

/** Feedback: the picture its input had last frame (TouchDesigner's Feedback; VCV Rack's one-step cable): the only
 *  way round a picture cycle. */
export const FEEDBACK = declared('feedback', 'Feedback', 'module', [], [PICTURE_IN], [PICTURE_OUT]);

/** The screen: what reaches it is drawn, through its flash guard (three flashes a second over the area the owner chose:
 *  fixed, never learned, never removed; `guard` how far it is holding now). */
export const SCREEN = declared('screen', 'Screen', 'finish', [], [PICTURE_IN], [out('guard', 'Flash guard')]);

export const GRAPH_MODULES: readonly Declared[] = [MIX, FEEDBACK, SCREEN];

/** Whether a picture cycle passes through a Feedback, as every one must (§4): the cables' picture graph, by module. */
export function cyclesClosed(p: PatchV2, typeOf: (type: string) => (ModuleType & { ports?: Ports }) | undefined): boolean {
  const type = new Map(p.modules.map((m) => [m.id, m.type]));
  const isPicture = (c: CableV2) => {
    const t = typeOf(type.get(c.from.module) ?? '');
    return !!t && portsOf(t).out.find((x) => x.id === c.from.port)?.type === 'picture';
  };
  const next = new Map<string, string[]>();
  for (const c of p.cables) if (isPicture(c) && type.get(c.from.module) !== 'feedback') next.set(c.from.module, [...(next.get(c.from.module) ?? []), c.to.module]);
  const state = new Map<string, 1 | 2>();
  const visit = (id: string): boolean => {
    if (state.get(id) === 2) return true;
    if (state.get(id) === 1) return false;
    state.set(id, 1);
    for (const n of next.get(id) ?? []) if (!visit(n)) return false;
    state.set(id, 2);
    return true;
  };
  return p.modules.every((m) => visit(m.id));
}
