// Typed ports (decision 0057, step 1): the interfaces the one engine (claude-5f), the picture graph (claude-632) and the
// views build on. Three kinds of signal: a picture (a texture each frame), a value (a number each frame), a trigger
// (events with their strength). A module type's ports come from what it declares: its knobs are value inputs, a
// modulator's `inputs` are value inputs, a picture module takes and gives a picture, a modulator gives a value. The
// modules that hold today's hidden behaviours are declared here, so every one of them shows.
import type { RouteValue } from './curves';
import { CLOCK_TRIGGER_KNOBS } from './engine/clock';
import { BUSY, type Knob, type Macro, type Module, type ModuleType, type Place } from './patch';
import { BAND_SIGNALS, TRACK_SIGNALS } from './routes';

/** `light`: the flame's accumulated light (its RGBA and its bands' shares) before Show turns it into a picture; the
 *  flame's own loop works on it (claude-632, step 3). Only Show converts light into a picture. */
/** `audio`: the sound itself, its recent samples (Music gives it; the Waves source draws it). */
/** `points`: a set of positions with values per point (a Grid's; for Instance). `flow`: a velocity field, where the
 *  picture moves and how fast (a Fluid's: 0057 §4h). */
export type PortType = 'picture' | 'light' | 'audio' | 'points' | 'value' | 'trigger' | 'flow';

/** A port: its id on its module, its name, its type; `channels`: a band's six at once (VCV's polyphonic cable; a
 *  cable may take one of them); `normal`: what it is wired to unless a cable is plugged in (Eurorack's normalled
 *  connection: shown faint, broken by a cable). */
export type Port = { id: string; label: string; type: PortType; channels?: number; normal?: PortRef; polymorphic?: Polymorphic };

/** A polymorphic port takes any of its types, its output following its input (as VCV's modules take any voltage):
 *  `true` is light or picture; a list names them (Fluid's: light, picture or points). */
export type Polymorphic = boolean | readonly PortType[];
const PICTURES: readonly PortType[] = ['light', 'picture'];

/** One end of a cable: a module's port, a channel of it where it has several. */
export type PortRef = { module: string; port: string; channel?: number };

export type Ports = { in: Port[]; out: Port[] };

/** The signals that are events: a beat, a band's hits. Into a value port they come as a pulse. */
export const TRIGGER_SIGNALS: ReadonlySet<string> = new Set(['beat', 'downbeat', 'section', 'hits', 'percussiveHits']);

/** Whether a cable may go from a port of `from` type into one of `to` type: the same, or a trigger into a value; into
 *  or out of a polymorphic port, any two of its types. */
export const fits = (from: PortType, to: PortType, polymorphic: Polymorphic = false) => {
  if (from === to || (from === 'trigger' && to === 'value')) return true;
  const types = polymorphic === true ? PICTURES : polymorphic || [];
  return types.includes(from) && types.includes(to);
};

const knobPort = (k: Knob): Port => ({ id: k.id, label: k.label, type: 'value' });
const PICTURE_IN: Port = { id: 'picture', label: 'Picture', type: 'picture' };
const PICTURE_OUT: Port = { id: 'picture', label: 'Picture', type: 'picture' };
const VALUE_OUT: Port = { id: 'out', label: 'Out', type: 'value' };
const TAKES_PICTURE: ReadonlySet<Place> = new Set<Place>(['module', 'loop', 'finish']);

/** A type's per-band knobs as today's chain keeps them (`lighting.0` … `lighting.5`) as one polyphonic knob each
 *  (0057 §4b): `lighting`, six channels, its values kept as they were. */
export function polyKnobs(t: ModuleType): ModuleType {
  const out: Knob[] = [];
  const seen = new Map<string, Knob>();
  for (const k of t.knobs) {
    const m = /^(.+)\.(\d+)$/.exec(k.id);
    if (!m) {
      out.push(k);
      continue;
    }
    const was = seen.get(m[1]);
    if (was) was.channels = Math.max(was.channels ?? 1, Number(m[2]) + 1);
    else {
      const poly: Knob = { ...k, id: m[1], label: k.label.replace(/\s*\d+$/, ''), channels: Number(m[2]) + 1 };
      seen.set(m[1], poly);
      out.push(poly);
    }
  }
  return { ...t, knobs: out };
}

const knobPortOf = (k: Knob): Port => ({ ...knobPort(k), ...(k.channels ? { channels: k.channels } : {}) });
/** A knob's gain (0057 §7e), normalled to 1: a level's value times it (a module's CV-controlled level); a swing's
 *  modulation (its cables' sum) times it (Bitwig's modulation amount); a time's value divided by it (a time CV). A VCA's
 *  or a Ducker's out is cabled in. Steps have none. */
const gainPortOf = (k: Knob): Port[] => (k.kind === 'step' ? [] : [{ id: `${k.id}:gain`, label: `${k.label}: gain`, type: 'value', ...(k.channels ? { channels: k.channels } : {}) }]);
/** A step knob's trigger (0057 §7e): it moves on to its next step at its module's neutral, as a Gate fires it. */
const nextPortOf = (k: Knob): Port[] => (k.kind === 'step' && k.randomize !== false ? [{ id: `${k.id}:next`, label: `${k.label}: next`, type: 'trigger' }] : []);

/** Trigger inputs a type takes beside its own (0057 §4b): the LFO, clocked (its rate in beats while its clock is
 *  plugged in) and reset on the bar. */
const TRIGGER_INPUTS: Record<string, Port[]> = {
  lfo: [
    { id: 'clock', label: 'Clock', type: 'trigger' },
    { id: 'reset', label: 'Reset', type: 'trigger', normal: { module: 'clock', port: 'bar' } },
  ],
};

/** A module type's ports, from what it declares (its own `ports` where it has them: the modules below). */
export function portsOf(t: ModuleType & { ports?: Ports }): Ports {
  if (t.ports) return t.ports;
  const declared = polyKnobs(t).knobs.filter((k) => !k.device);
  const knobs = [...declared.map(knobPortOf), ...declared.flatMap(nextPortOf), ...declared.flatMap(gainPortOf)];
  const inputs = [
    ...(t.inputs ?? []).map((id): Port => ({ id, label: id === 'in' ? 'Input' : id[0].toUpperCase() + id.slice(1), type: id === 'trigger' ? 'trigger' : 'value' })),
    ...(TRIGGER_INPUTS[t.type] ?? []),
  ];
  if (t.place === 'modulator') return { in: [...inputs, ...knobs], out: [VALUE_OUT] };
  if (t.place === 'frame') return { in: knobs, out: [] };
  // The flame and its own loop work on light (claude-632, step 3): the flame takes last frame's light and gives its
  // light; Trails, Fluid, Tunnel, the Loop effect and Dive carry light on.
  // The flame's part columns take six channels, one a part (0057 §4b): a band's cable reaches its part; a whole
  // track's every part. Into any one-channel knob, six channels are the bands' mean (a Merge, in the conversion).
  // The flame gives where its parts are too (0057 §4h: where the Fluid's pushes land, unless cabled otherwise).
  if (t.type === 'flame')
    return { in: [LIGHT('light', 'Last light'), ...knobs.map((p) => (FLAME_PARTS.has(p.id) ? { ...p, channels: 6 } : p))], out: [LIGHT('light', 'Light'), { id: 'points', label: 'Points', type: 'points' }] };
  // Fluid (0057 §4h, the owner): light, a picture or points through its flow, the same type out; its pushes at the
  // points into `at` (the flame's parts unless cabled); its flow out, for Show's read-back, Stream and Displace.
  if (t.type === 'fluid' || t.type === 'fluid-loop')
    return {
      in: [{ ...LIGHT('light', 'In'), polymorphic: FLUID_TYPES }, { id: 'at', label: 'Pushes at', type: 'points', normal: { module: 'flame', port: 'points' } }, ...knobs],
      out: [{ ...LIGHT('light', 'Out'), polymorphic: FLUID_TYPES }, FLOW_OUT],
    };
  // The Loop effect's Stream reads a flow (the fluid's unless cabled).
  if (t.type === 'loop-effect') return { in: [LIGHT('light', 'Light'), FLOW_IN, ...knobs], out: [LIGHT('light', 'Light')] };
  if (LIGHT_CARRIERS.has(t.type)) return { in: [LIGHT('light', 'Light'), ...knobs], out: [LIGHT('light', 'Light')] };
  // Hydra's Stream too.
  if (t.type === 'hydra-engine') return { in: [PICTURE_IN, FLOW_IN, ...knobs], out: [PICTURE_OUT] };
  return { in: [...(TAKES_PICTURE.has(t.place) ? [PICTURE_IN] : []), ...knobs], out: [PICTURE_OUT, ...(POINTS_OUT[t.type] ?? [])] };
}

/** The sources whose geometry is an output too (0057 §4g): Regular shapes' corners, for Instance (a corner fractal:
 *  corners → Instance → Feedback). */
const POINTS_OUT: Record<string, Port[]> = { shapes: [{ id: 'corners', label: 'Corners', type: 'points' }] };

const LIGHT = (id: string, label: string): Port => ({ id, label, type: 'light' });
const FLUID_TYPES: readonly PortType[] = ['light', 'picture', 'points'];
// Its flow field (`field`: the Fluid's Flow is a knob, how much its cables make it flow).
const FLOW_OUT: Port = { id: 'field', label: 'Flow field', type: 'flow' };
/** A flow field input, normalled to the Fluid's (the flame's own loop's). */
const FLOW_IN: Port = { id: 'field', label: 'Flow field', type: 'flow', normal: { module: 'fluid', port: 'field' } };
/** The flame's part columns: polyphonic inputs, a part a channel. */
export const FLAME_PARTS: ReadonlySet<string> = new Set(['shape', 'rotation', 'lights', 'hue', 'spread', 'presence']);
/** The flame's own loop, on its light (Dive last: a pass-through whose rate the points' camera reads). */
export const LIGHT_CARRIERS: ReadonlySet<string> = new Set(['trails', 'fluid', 'tunnel', 'loop-effect', 'dive']);

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
const MUSIC_KNOBS: Knob[] = [
  { ...knob('hold', 'Hold', 0.02, 1, 0.15), channels: 6 },
  // How each row is read (0057 §7e item 10): 0 against this track's typical, 1 lately's, 2 as it is.
  ...[...TRACK_SIGNALS, ...BAND_SIGNALS].map((s) => ({ ...knob(`reference.${s}`, `${label(s)}: reading`, 0, 2, 0, 'step'), randomize: false as const })),
];

export const MUSIC: Declared = {
  type: 'music',
  label: 'Music',
  place: 'modulator',
  // A band's envelope's fall (s): its hold, what today's lighting times gave the envelope (claude-5f).
  knobs: MUSIC_KNOBS,
  ports: {
    in: MUSIC_KNOBS.map(knobPortOf),
    out: [
      out('audio', 'Sound', 'audio'),
      // 1 while the music plays, 0 paused (0057 §7e: the pause's dimming, cabled in sight).
      out('playing', 'Playing'),
      // A trigger as a track starts (the visuals opening on one too), and the track's seed (0..1, the same for the same
      // track): Track folded into Music (0057 §7f).
      out('newtrack', 'New track', 'trigger'),
      out('seed', 'Track seed'),
      ...TRACK_SIGNALS.map((s) => out(s, label(s), TRIGGER_SIGNALS.has(s) ? 'trigger' : 'value')),
      ...BAND_SIGNALS.map((s) => out(s, label(s), TRIGGER_SIGNALS.has(s) ? 'trigger' : 'value', 6)),
      // Each value signal's slow form (the speed a cable no longer says).
      ...TRACK_SIGNALS.filter((s) => !TRIGGER_SIGNALS.has(s)).map((s) => out(`${s}:slow`, `${label(s)} (slow)`)),
      ...BAND_SIGNALS.filter((s) => !TRIGGER_SIGNALS.has(s)).map((s) => out(`${s}:slow`, `${label(s)} (slow)`, 'value', 6)),
    ],
  },
};

export const TRACK: Declared = {
  type: 'track',
  label: 'Track',
  place: 'modulator',
  knobs: [],
  ports: { in: [], out: [out('pulse', 'New track'), out('seed', 'Seed')] },
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
  [knob('speed', 'Speed', 0, 2, 1, 'level', 0), knob('beats', 'Beats a turn', 4, 128, 32), ...CLOCK_TRIGGER_KNOBS],
  [{ id: 'tempo', label: 'Tempo', type: 'value', normal: { module: 'music', port: 'tempo' } }],
  [out('phase', 'Turn'), out('rate', 'Rate'), out('beat', 'Beat', 'trigger'), out('bar', 'Bar', 'trigger'), out('phrase', 'Phrase', 'trigger')],
);

/** Accent (0057 §7e; was Flash, renamed so "flash" means only the Screen's guard): a slow course of its input, and
 *  the accent's share of the swing about it. */
export const ACCENT = declared(
  'accent',
  'Accent',
  'modulator',
  [knob('accent', 'Accent', 0, 2, 1, 'level', 0), knob('course', 'Course', 0.05, 2, 0.3)],
  [{ id: 'in', label: 'Input', type: 'value' }],
  [VALUE_OUT],
);

/** VCA (0057 §7e): its input times its gain, as in every modular synth; the one way a signal multiplies another. */
export const VCA = declared('vca', 'VCA', 'modulator', [knob('gain', 'Gain', 0, 2, 1, 'level', 0)], [{ id: 'in', label: 'Input', type: 'value' }, { id: 'cv', label: 'Gain in', type: 'value' }], [VALUE_OUT]);

/** The ways a Merge makes one channel of several. */
export const MERGES = ['sum', 'mean', 'max', 'geometric'] as const;

/** Merge (0057 §7e; VCV's poly to mono): a polyphonic cable's channels made one, by their sum, mean, largest or
 *  geometric mean. */
export const MERGE = declared(
  'merge',
  'Merge',
  'modulator',
  [{ ...knob('mode', 'Mode', 0, MERGES.length - 1, 1, 'step'), randomize: false }],
  [{ id: 'in', label: 'Input', type: 'value', channels: 6 }],
  [VALUE_OUT],
);


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
  [
    ...[knob('attack', 'Attack', 0, 2, 0.04), knob('decay', 'Decay', 0, 4, 0), knob('sustain', 'Sustain', 0, 1, 0, 'level'), knob('release', 'Release', 0.02, 8, 0.2)].map((k) => ({ ...k, channels: 6 })),
    // How a hit that comes while one rings starts: from the higher of the two (0, as today's lights), or from where it
    // is (1, the textbook's).
    { ...knob('retrigger', 'Retrigger', 0, 1, 0, 'step'), randomize: false },
    knob('amount', 'Amount', 0, 1, 1, 'level'),
    // How it reads a strike's strength (READINGS): the reading its gate's cable had.
    { ...knob('reference', 'Reading', 0, 2, 0, 'step'), randomize: false },
  ],
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

/** The Show device's modules (0057 §6), in the order that draws exactly today's picture (claude-632, render.ts's
 *  show pass): each with the knob it took from Show (id and type alike). Bend and Blur take light or a picture
 *  (polymorphic); Tone map turns light into a picture; the compositor fuses the five into one pass. Show's Size is
 *  the Flame's own zoom, not here. */
export const SHOW_PARTS: readonly { knob: string; type: string; label: string; polymorphic?: boolean }[] = [
  { knob: 'waves', type: 'bend', label: 'Bend', polymorphic: true },
  { knob: 'smoothing', type: 'blur', label: 'Blur', polymorphic: true },
  { knob: 'glow', type: 'tonemap', label: 'Tone map' },
  { knob: 'haze', type: 'curve', label: 'Curve' },
  { knob: 'saturation', type: 'saturate', label: 'Saturate' },
];

/** Show's parts as module types, each with its knob as Show's type has it (`show`: patch.ts cratarrTypes's). */
export function showPartTypes(show: ModuleType): Declared[] {
  return SHOW_PARTS.map(({ knob: id, type, label: name, polymorphic }) => {
    const k = show.knobs.find((x) => x.id === id);
    // Tone map's Level (0057 §7e item 6): the whole picture's brightness as a fader, 1 as set; the pause dims it
    // through its gain.
    const knobs = [...(k ? [k] : []), ...(type === 'tonemap' ? [knob('level', 'Level', 0, 1, 1, 'level')] : [])];
    const input: Port = type === 'tonemap' ? { id: 'light', label: 'Light', type: 'light' } : polymorphic ? { ...PICTURE_IN, polymorphic } : PICTURE_IN;
    // Bend reads the picture along the Fluid's flow (0057 §4h: today's hidden read-back), the fluid's unless cabled.
    const inputs = type === 'bend' ? [input, FLOW_IN] : [input];
    return declared(type, name, 'module', knobs, inputs, [polymorphic ? { ...PICTURE_OUT, polymorphic } : PICTURE_OUT]);
  });
}

/** The swing columns of today's chain as v2 has them (0057 §7e item 2, VCV's rule): the column's resting value, the
 *  cables adding to it; bipolar about 0 (a full cable gives ±1 in the same units). Their scale went into the cables. */
export const COLUMN_OFFSETS: ReadonlySet<string> = new Set(['flame.shape', 'flame.rotation', 'flame.lights', 'flame.presence', 'flame.zoom', 'trails.trails', 'tonemap.glow', 'saturate.saturation', 'bend.waves']);

/** Today's chain's types as v2 has them: the swing columns as offsets (−1..1, at 0). */
export function v2ChainType(t: ModuleType): ModuleType {
  return { ...t, knobs: t.knobs.map((k) => (COLUMN_OFFSETS.has(`${t.type}.${k.id}`) ? { ...k, min: -1, max: 1, neutral: 0, default: 0, kind: 'swing' as const } : k)) };
}

/** The lattices a Grid lays its points on (0057 §4c). */
export const LATTICES = ['square', 'hexagonal', 'triangular', 'rings', 'spiral'] as const;
/** The most points a Grid gives (its channels). */
export const MOST_POINTS = 64;

/** Grid (0057 §4c; TouchDesigner's Grid SOP): points on a lattice, and each point's values as channels. Its frame is
 *  centred, y up, the screen's short side −1..1 (as Fold and Instance read it). */
export const GRID = declared(
  'grid',
  'Grid',
  'modulator',
  [
    knob('lattice', 'Lattice', 0, LATTICES.length - 1, 0, 'step'),
    knob('count', 'Count', 1, MOST_POINTS, 16, 'level', 1),
    knob('spacing', 'Spacing', 0.02, 1, 0.25),
    knob('rotation', 'Rotation', 0, 1, 0),
    knob('jitter', 'Jitter', 0, 1, 0, 'level'),
  ],
  [],
  [
    out('points', 'Points', 'points'),
    out('index', 'Index', 'value', MOST_POINTS),
    out('distance', 'Distance', 'value', MOST_POINTS),
    out('angle', 'Angle', 'value', MOST_POINTS),
    out('row', 'Row', 'value', MOST_POINTS),
    out('column', 'Column', 'value', MOST_POINTS),
  ],
);

/** The symmetries a Fold applies (0057 §4c): Hydra's three first, the wallpaper groups beyond. */
export const FOLDS = ['mosaic', 'kaleidoscope', 'mirror', 'pixelate'] as const;

/** Fold (0057 §4c): a picture folded by a grid's symmetry; with no grid plugged in, its own centre and count. */
export const FOLD = declared(
  'fold',
  'Fold',
  'module',
  [knob('mode', 'Mode', 0, FOLDS.length - 1, 1, 'step'), knob('count', 'Count', 1, 24, 6), knob('turn', 'Turn', -1, 1, 0), knob('amount', 'Amount', 0, 1, 1, 'level')],
  [PICTURE_IN, { id: 'grid', label: 'Grid', type: 'points' }],
  [PICTURE_OUT],
);

/** Instance (0057 §4c; TouchDesigner's and cables.gl's instancing): its source drawn at each point. */
export const INSTANCE = declared(
  'instance',
  'Instance',
  'module',
  [knob('amount', 'Amount', 0, 1, 1, 'level')],
  [{ id: 'points', label: 'Points', type: 'points' }, PICTURE_IN],
  [PICTURE_OUT],
);

/** Filter bank (0057 §4b): the sound split into six bands by a crossover (Linkwitz–Riley), a band a channel. Its five
 *  split points (Hz) movable. */
export const FILTER_BANK = declared(
  'filterbank',
  'Filter bank',
  'modulator',
  [60, 250, 500, 2000, 6000].map((hz, i) => knob(`split${i + 1}`, `Split ${i + 1}`, 20, 16000, hz)),
  [{ id: 'audio', label: 'Sound', type: 'audio', normal: { module: 'music', port: 'audio' } }],
  [out('bands', 'Bands', 'audio', 6), out('level', 'Level', 'value', 6)],
);

/** The kind of busyness each knob makes (0057 §7e, claude-5f's sidechain): the Ducker of that kind holds it back. */
export const BUSY_OF: Readonly<Record<string, (typeof BUSY)[number]>> = {
  'clock.speed': 'motion',
  'flame.hue': 'colour',
  'flame.zoom': 'zoom',
  'flame.evolution': 'shape',
  'gate.gap': 'looks',
};

/** Ducker (0057 §7e; the broadcast sidechain, claude-5f's): its input lowered by up to Depth (dB) while its key stands
 *  over the threshold (attack, hold, release; let go under Hysteresis × the threshold). For counts (flashes a second,
 *  looks a minute), which a ratio compressor in a closed loop cannot settle. Its input unplugged is 1. */
export const DUCKER = declared(
  'ducker',
  'Ducker',
  'modulator',
  [
    { ...knob('threshold', 'Threshold', 0.001, 10, 1), taper: 'exp' as const },
    knob('depth', 'Depth (dB)', 0, 40, 12),
    knob('attack', 'Attack', 0, 5, 1),
    knob('hold', 'Hold', 0, 10, 0),
    knob('release', 'Release', 0, 20, 3),
    knob('hysteresis', 'Hysteresis', 0, 1, 0.8),
  ].map((k) => ({ ...k, randomize: false as const })),
  [
    { id: 'key', label: 'Sidechain', type: 'value' },
    { id: 'in', label: 'Input', type: 'value' },
  ],
  [VALUE_OUT],
);

/** The picture modules of 0057 §5 and §6b (claude-632's ids): `picture` in and out; `by` a second picture. */
const BY: Port = { id: 'by', label: 'By', type: 'picture' };
export const GLOW = declared('glow', 'Glow', 'module', [knob('glow', 'Glow', 0, 1, 0.4, 'level')], [PICTURE_IN], [PICTURE_OUT]);
export const TINT = declared('tint', 'Tint', 'module', [knob('tint', 'Tint', 0, 1, 0.7, 'level')], [PICTURE_IN, BY], [PICTURE_OUT]);
export const MASK = declared('mask', 'Mask', 'module', [knob('amount', 'Amount', 0, 1, 1, 'level'), knob('invert', 'Invert', 0, 1, 0, 'step')], [PICTURE_IN, BY], [PICTURE_OUT]);
export const DISPLACE = declared(
  'displace',
  'Displace',
  'module',
  [knob('amount', 'Amount', 0, 1, 0.3, 'level'), knob('scale', 'Scale', 0, 1, 0.5), knob('mode', 'Mode', 0, 1, 0, 'step')],
  [PICTURE_IN, BY],
  [PICTURE_OUT],
);
/** Threshold: a luma key, only the picture's bright parts (for Bloom). */
export const THRESHOLD = declared('threshold', 'Threshold', 'module', [knob('threshold', 'Threshold', 0, 1, 0.6, 'level'), knob('softness', 'Softness', 0, 1, 0.1)], [PICTURE_IN], [PICTURE_OUT]);
export const PICTURE_MODULES: readonly Declared[] = [GLOW, TINT, MASK, DISPLACE, THRESHOLD];

/** The Randomize scopes (0057 §4d), in the dice's menu's order. */
export const RANDOMIZE_SCOPES = ['everything', 'sources', 'effects', 'mixers', 'modulators', 'cables'] as const;

/** Dice (0057 §7f, the owner: "same randomize"; named Dice beside Random, the value generator): the panel's dice as a
 *  module. A trigger randomizes every knob in its
 *  scope that is not pinned, as the dice do (the flame's settings rolled together and checked), from its seed: the same
 *  seed, the same values. Music's New track and Track seed into it are a track's look (the Track Look device). */
export const DICE = declared(
  'dice',
  'Dice',
  'modulator',
  [{ ...knob('scope', 'Scope', 0, RANDOMIZE_SCOPES.length - 1, 0, 'step'), randomize: false }],
  [
    { id: 'trigger', label: 'Trigger', type: 'trigger' },
    { id: 'seed', label: 'Seed', type: 'value' },
  ],
  // A trigger as it randomizes (to morph or flash with it).
  [out('done', 'Randomized', 'trigger')],
);

/** Shaper (0057 §7f, the owner: curved cables kept exact): a value through a curve, as a cable's own curve shapes it
 *  (curves.ts: the end's height and two handles). A curved cable from Music reads it before the loudness's VCA. A shape
 *  is a choice, not a value: none of its knobs are randomized. */
export const SHAPER = declared(
  'shaper',
  'Shaper',
  'modulator',
  [
    { ...knob('h', 'Height', -2, 2, 1), randomize: false },
    { ...knob('h1x', 'Handle 1 across', 0, 1, 1 / 3), randomize: false },
    { ...knob('h1y', 'Handle 1 height', -2, 2, 1 / 3), randomize: false },
    { ...knob('h2x', 'Handle 2 across', 0, 1, 2 / 3), randomize: false },
    { ...knob('h2y', 'Handle 2 height', -2, 2, 2 / 3), randomize: false },
  ],
  [{ id: 'in', label: 'In', type: 'value', channels: 6 }],
  [{ ...VALUE_OUT, channels: 6 }],
);

export const ENGINE_MODULES: readonly Declared[] = [MUSIC, TRACK, ENVELOPE, DICE, SHAPER, CLOCK, ACCENT, VCA, MERGE, METER, COMPRESSOR, DUCKER, ADSR, SEQUENCER, LOOPER, GRID, FOLD, INSTANCE, FILTER_BANK];

// ---- The v2 patch (0057 §4, §7): only modules and cables; layers, loops and the finish are views of the wiring ------

/** A cable between typed ports. From a signal port of Music it keeps 0056's reference and speed; its `depth` is the
 *  port that scales it (undefined: normalled to the Envelope; null: unplugged). */
/** A cable (0057 §7e item 10, the owner): its amount (a curve; the plain amount the straight one), its depth (what
 *  scales it: undefined normalled to the Loudness's out, null none, else that port), its channel (one band, in its
 *  ends). It only adds. How a signal is read is its Music row's; its slow form a Music output of its own. */
export type CableV2 = {
  id: string;
  from: PortRef;
  to: PortRef;
  value: RouteValue;
  depth?: PortRef | null;
};

/** How a Music row reads its signal (0057, "reference per row"): against this track's typical, lately's, or as it is. */
export const READINGS = ['track', 'lately', 'absolute'] as const;

export type PatchV2 = { v: 2; modules: Module[]; cables: CableV2[]; macros: Macro[]; pins: string[] };

/** The blends a Mix input takes (0056's, as a step knob per input). */
// Appended only: a saved Mix keeps its modes by their place (Subtract, 0057 §4: a picture less another, its high-pass
// a picture less its blur).
export const MIX_MODES = ['add', 'screen', 'over', 'multiply', 'window', 'lighten', 'subtract'] as const;
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
 *  way round a picture cycle. Light or a picture (the flame's own loop runs on light). */
export const FEEDBACK = declared('feedback', 'Feedback', 'module', [], [{ ...PICTURE_IN, polymorphic: true }], [{ ...PICTURE_OUT, polymorphic: true }]);

/** The screen: what reaches it is drawn, through its flash guard (three flashes a second over the area the owner chose:
 *  fixed, never learned, never removed; `guard` how far it is holding now). */
export const SCREEN = declared('screen', 'Screen', 'finish', [], [PICTURE_IN], [out('guard', 'Flash guard')]);

export const GRAPH_MODULES: readonly Declared[] = [MIX, FEEDBACK, SCREEN];

/** Whether a picture cycle passes through a Feedback, as every one must (§4): the cables' picture and light graph, by
 *  module (the flame's own loop is light). */
export function cyclesClosed(p: PatchV2, typeOf: (type: string) => (ModuleType & { ports?: Ports }) | undefined): boolean {
  const type = new Map(p.modules.map((m) => [m.id, m.type]));
  const isPicture = (c: CableV2) => {
    const t = typeOf(type.get(c.from.module) ?? '');
    const kind = t ? portsOf(t).out.find((x) => x.id === c.from.port)?.type : undefined;
    return kind === 'picture' || kind === 'light';
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

// ---- Devices (0057 §6): a group of modules as one node, after Ableton's Racks and Reaktor's macros ----------------

/** A port of a device: one of its modules' ports, shown on the device (`as`: its id on the device). */
export type ExposedPort = PortRef & { as: string; label: string };

/** A device's macro: a knob on the device moving chosen knobs of its modules (0056's Macro, kept inside it). */
export type DeviceMacro = { id: string; label: string; value: number; targets: { module: string; knob: string; from: number; to: number }[] };

/** A device in a patch: a module of type `device` holding a patch of its own; its exposed ports and its macros. The
 *  engine and the compositor flatten it before running. Nests: its patch may hold devices. */
export type Device = Module & { type: 'device'; label: string; patch: PatchV2; inputs: ExposedPort[]; outputs: ExposedPort[]; macros: DeviceMacro[] };

export const isDevice = (m: Module): m is Device => m.type === 'device';
