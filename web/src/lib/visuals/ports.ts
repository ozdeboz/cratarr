// Typed ports (decision 0057, step 1): the interfaces the one engine (claude-5f), the picture graph (claude-632) and the
// views build on. Three kinds of signal: a picture (a texture each frame), a value (a number each frame), a trigger
// (events with their strength). A module type's ports come from what it declares: its knobs are value inputs, a
// modulator's `inputs` are value inputs, a picture module takes and gives a picture, a modulator gives a value. The
// modules that hold today's hidden behaviours are declared here, so every one of them shows.
import type { Knob, ModuleType, Place } from './patch';
import { BAND_SIGNALS, TRACK_SIGNALS } from './routes';

export type PortType = 'picture' | 'value' | 'trigger';

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
  'Envelope',
  'modulator',
  [knob('intensity', 'Intensity', 0, 1, 0.5, 'level', 0), knob('time', 'Time', 0.02, 2, 0.2)],
  [{ id: 'in', label: 'Input', type: 'value', normal: { module: 'music', port: 'loudness' } }],
  [VALUE_OUT],
);

export const CLOCK = declared(
  'clock',
  'Clock',
  'modulator',
  [knob('speed', 'Speed', 0, 2, 1, 'level', 0), knob('beats', 'Beats a turn', 4, 128, 32)],
  [{ id: 'tempo', label: 'Tempo', type: 'value', normal: { module: 'music', port: 'tempo' } }],
  [out('phase', 'Turn'), out('rate', 'Rate')],
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

/** The flash meter and the governor: fixed, last before the screen, never removed; Wild sets its limits. */
export const SAFETY = declared('safety', 'Safety', 'finish', [knob('wild', 'Wild', 0, 1, 0.5)], [PICTURE_IN], [PICTURE_OUT, out('holding', 'Holding back')]);

export const ENGINE_MODULES: readonly Declared[] = [MUSIC, TRACK, ENVELOPE, CLOCK, FLASH, WALK, SAFETY];
