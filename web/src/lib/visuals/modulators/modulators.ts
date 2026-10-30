// The visuals' modulators (decision 0056): modules with no picture, whose outputs feed cables as the music's signals do.
// The set is what curated MilkDrop presets build by hand before they drive anything: summed volume, a fast follower and
// the Envelope Follower, moving averages, peak and mean trackers, "above typical" beat gates, counters, LFOs and random,
// sample-and-hold.
//
// - Inputs are cables, as in a modular: the engine sums the cables into each port (`inputs`) and passes the sum as that
//   knob (`knobs.in`; 0 where nothing is cabled), so they chain: a follower into a gate into a counter. Volume alone
//   reads the bands' signals itself.
// - Each gives 0..1 (`range: 'unipolar'`) or −1..1 ('bipolar'). A signal straight from the music is 1 = typical.
// - Every knob is continuous, and with every knob at its neutral the output is 0 (Amount at 0, the rest at "does
//   nothing"), so a chain change fades a modulator in from its neutral or out to it.
// - Times are seconds and smoothing is exponential over dt, so 30 and 120 frames a second give the same.
//
// Pure: no WebGL, no DOM.

import { RATIOS, UNIT_RATIO, ratioOf } from '../engine/clock';
import type { Knob, Modulator, ModuleType, SignalRef } from '../patch';
import { withAspects } from '../engine/knob-aspects';

/** The music's signals now, 1 = typical. */
export type Signals = (ref: SignalRef) => number;

/** A modulator's module type: what it gives (0..1 or −1..1), its input ports, and a new running one. */
export type ModulatorType = ModuleType & {
  place: 'modulator';
  range: 'unipolar' | 'bipolar';
  inputs?: string[];
  /** Its trigger inputs (ports.ts TRIGGER_INPUTS declares their ports): each 1 on the frame it fires, else 0, given
   *  only where something is plugged in or normalled (left out of `set`, or not a number: nothing is). */
  triggers?: string[];
  create(seed?: number): Modulator;
};

/** The bands a track's signals have (signals.ts). */
export const BANDS = 6;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
/** What is left of a gap after `dt` of an exponential approach with time constant `tau` (nothing at 0). */
const keep = (dt: number, tau: number) => (tau > 0 ? Math.exp(-dt / tau) : 0);
const frac = (u: number) => u - Math.floor(u);
const finite = (v: number | undefined) => (typeof v === 'number' && Number.isFinite(v) ? v : 0);

const knob = (id: string, label: string, min: number, max: number, neutral: number, def: number, kind: Knob['kind'] = 'swing'): Knob => ({
  id,
  label,
  min,
  max,
  neutral,
  default: def,
  kind,
});
const AMOUNT = knob('amount', 'Amount', 0, 1, 0, 1, 'level');
/** The input that reads as 1 (at 2, a typical signal is half way). */
const FULL = knob('full', 'Full at', 0.25, 4, 1, 2);
const unit = (x: number, full: number) => clamp(x / full, 0, 1);
const IN = ['in'];

/** Each knob as set, within its range, its default where it is not set; each port's sum as it came (0 uncabled); each
 *  trigger as it came, NaN where nothing is plugged in. */
function settled(knobs: Knob[], ports: string[], triggers: string[], set: Record<string, number>, into: Record<string, number>) {
  for (const k of knobs) {
    const v = set[k.id];
    into[k.id] = Number.isFinite(v) ? clamp(v, k.min, k.max) : k.default;
  }
  for (const p of ports) into[p] = finite(set[p]);
  for (const p of triggers) into[p] = Number.isFinite(set[p]) ? set[p] : NaN;
  return into;
}

type Step = (dt: number, signals: Signals, k: Record<string, number>) => number;

function define(
  type: string,
  label: string,
  range: ModulatorType['range'],
  knobs: Knob[],
  inputs: string[] | undefined,
  make: (seed: number) => Step,
  triggers?: string[],
): ModulatorType {
  const [lo, hi] = range === 'bipolar' ? [-1, 1] : [0, 1];
  return {
    type,
    label,
    place: 'modulator',
    range,
    // How a modulator follows means none of the aspect dice's four, but what one declares (a rate: how fast what it
    // moves moves).
    knobs: withAspects(knobs, {}, null),
    ...(inputs ? { inputs } : {}),
    ...(triggers ? { triggers } : {}),
    create(seed = 0): Modulator {
      const step = make(seed);
      const k: Record<string, number> = {};
      return {
        step(dt, signals, set) {
          const v = step(Number.isFinite(dt) ? Math.max(0, dt) : 0, signals, settled(knobs, inputs ?? [], triggers ?? [], set, k));
          return Number.isFinite(v) ? clamp(v, lo, hi) : 0;
        },
      };
    },
  };
}

// MilkDrop's vol: the bands' levels together, each by its weight (their mean once the weights reach 1, so all six at 1
// read as typical at 1).
const volume = define(
  'volume',
  'Volume',
  'unipolar',
  [...Array.from({ length: BANDS }, (_, b) => knob(`band${b + 1}`, `Band ${b + 1}`, 0, 1, 0, 1, 'level')), FULL, AMOUNT],
  undefined,
  () => {
    const refs: SignalRef[] = Array.from({ length: BANDS }, (_, band) => ({ signal: 'level', band }));
    return (_dt, signals, k) => {
      let sum = 0;
      let weight = 0;
      for (let b = 0; b < BANDS; b++) {
        const w = k[`band${b + 1}`];
        if (w > 0) {
          sum += w * Math.max(0, finite(signals(refs[b])));
          weight += w;
        }
      }
      return k.amount * unit(sum / Math.max(1, weight), k.full);
    };
  },
);

// A follower: rises with the attack's time constant, falls with the release's.
const follower = (type: string, label: string, attack: number, release: number) =>
  define(type, label, 'unipolar', [knob('attack', 'Attack', 0, 2, 0, attack), knob('release', 'Release', 0, 5, 0, release), FULL, AMOUNT], IN, () => {
    let y = 0;
    return (dt, _signals, k) => {
      y = k.in + (y - k.in) * keep(dt, k.in > y ? k.attack : k.release);
      return k.amount * unit(y, k.full);
    };
  });
const fast = follower('fast', 'Fast follower', 0.01, 0.15);

/** The Envelope Follower's knobs (one type for the Loudness and the slow follower it replaced, 0057 §7e): its rise and
 *  fall (s), and Intensity, how far what it follows counts (1: all of it; 0: none, the output held at 1). */
export const ENVELOPE_KNOBS: Knob[] = [knob('attack', 'Attack', 0, 2, 0, 0.2), knob('release', 'Release', 0, 5, 0, 0.2), knob('intensity', 'Intensity', 0, 1, 1, 1)];
/** An Envelope Follower: its input followed (rising over its attack, falling over its release), as far as Intensity
 *  lets it count. The Loudness's, on Music's loudness, the engine steps itself (graph.ts); every other runs as this. */
const envelope = define('envelope', 'Envelope Follower', 'unipolar', ENVELOPE_KNOBS, IN, () => {
  let y = 0;
  return (dt, _signals, k) => {
    y = k.in + (y - k.in) * keep(dt, k.in > y ? k.attack : k.release);
    return 1 - k.intensity * (1 - clamp(y, 0, 1));
  };
});

// A moving average over about its window, from silence.
const average = define('average', 'Average', 'unipolar', [knob('window', 'Window', 0, 8, 0, 1), FULL, AMOUNT], IN, () => {
  let y = 0;
  return (dt, _signals, k) => {
    y = k.in + (y - k.in) * keep(dt, k.window);
    return k.amount * unit(y, k.full);
  };
});

// The peak: the highest lately, falling toward 0 with its time constant and caught again by anything above it.
const peak = define('peak', 'Peak', 'unipolar', [knob('fall', 'Fall', 0, 8, 0, 1.5), FULL, AMOUNT], IN, () => {
  let y = 0;
  return (dt, _signals, k) => {
    y = Math.max(k.in, y * keep(dt, k.fall));
    return k.amount * unit(y, k.full);
  };
});

// The mean of what has come in, forgetting what is older than about its horizon. Unlike Average it starts from the
// first values, not from silence (the weight heard so far divides it out).
const mean = define('mean', 'Mean', 'unipolar', [knob('horizon', 'Horizon', 0, 60, 0, 10), FULL, AMOUNT], IN, () => {
  let m = 0;
  let w = 0;
  return (dt, _signals, k) => {
    const left = keep(dt, k.horizon);
    m = k.in + (m - k.in) * left;
    w = 1 + (w - 1) * left;
    return k.amount * unit(w > 0 ? m / w : k.in, k.full);
  };
});

// The beat gate: opens when its input goes above Above (1: above typical, where MilkDrop's beat tests sit), closes
// below that less its Hysteresis share. Each opening lifts the next one's threshold by Lift, which fades over Recover,
// as presets' adaptive thresholds do. It opens no sooner than Gap after it last did (a step's moments: 2 s). Open it
// gives 1; shut it falls over Release.
const gate = define(
  'gate',
  'Beat gate',
  'unipolar',
  [
    knob('above', 'Above', 0.5, 3, 1, 1),
    knob('hysteresis', 'Hysteresis', 0, 0.9, 0, 0.15),
    knob('lift', 'Lift', 0, 2, 0, 0.3),
    knob('recover', 'Recover', 0, 4, 0, 0.5),
    knob('release', 'Release', 0, 2, 0, 0.1),
    { ...knob('gap', 'Gap', 0, 10, 0, 0), time: true },
    AMOUNT,
  ],
  IN,
  () => {
    let open = false;
    let lifted = 0;
    let y = 0;
    let since = Infinity;
    return (dt, _signals, k) => {
      lifted *= keep(dt, k.recover);
      since += dt;
      // More than Gap since it last opened (a frame exactly Gap on is not yet: a sum of frames' times is a hair off).
      if (!open && k.in > k.above + lifted && (k.gap <= 0 || since > k.gap + 1e-9)) {
        open = true;
        lifted += k.lift;
        since = 0;
      } else if (open && k.in < k.above * (1 - k.hysteresis)) open = false;
      y = open ? 1 : y * keep(dt, k.release);
      return k.amount * y;
    };
  },
);

/** A counter's input counts as it rises above this, and again once it has fallen below `RESET`. */
const RISE = 0.5;
const RESET = 0.4;

// A counter: each time its input rises above ½ (a gate's opening, a beat) it moves on by 1/Length and wraps at 1 (0, ¼,
// ½, ¾, 0 … at 4): beats into it count a bar, downbeats a phrase. Length moves the step, not the count, so it morphs
// without a jump; at 1 the count stays at 0.
const counter = define('counter', 'Counter', 'unipolar', [knob('length', 'Length', 1, 16, 1, 4), AMOUNT], IN, () => {
  let high = false;
  let phase = 0;
  return (_dt, _signals, k) => {
    if (!high && k.in > RISE) {
      high = true;
      phase = frac(phase + 1 / k.length);
      if (1 - phase < 1e-9) phase = 0;
    } else if (high && k.in < RESET) high = false;
    return k.amount * phase;
  };
});

/** The LFO's waves at a phase `p` (cycles), each −1..1 and starting at 0 rising, as the sine does: sine, triangle, saw,
 *  square (VCV's LFO's four). */
const WAVES: readonly ((p: number) => number)[] = [
  (p) => Math.sin(2 * Math.PI * p),
  (p) => 4 * Math.abs(frac(p - 0.25) - 0.5) - 1,
  (p) => 2 * frac(p + 0.5) - 1,
  (p) => (frac(p) < 0.5 ? 1 : -1),
];

// An LFO: Rate cycles a second, its wave by Shape (0 a sine, 1 a triangle, 2 a saw, 3 a square, gliding between
// neighbours), started Offset of a cycle in. Clocked (a trigger plugged into `clock`, ports.ts TRIGGER_INPUTS): Ratio
// cycles a clock tick instead of Rate (÷4 … ×4, clock.ts RATIOS, read at its nearest; continuous as every knob here,
// and a change of it changes the pace, never the phase), locked to the ticks: between two it runs on at the last
// two's period, holding at the next tick's place should that one be late, and a tick early moves it on to its place.
// `reset` (normalled to the Clock's bar) puts it back to the cycle's start. Unclocked, `reset` does nothing, so an LFO
// with nothing plugged in runs as it always has.
const lfo = define(
  'lfo',
  'LFO',
  'bipolar',
  [
    { ...knob('rate', 'Rate', 0, 8, 0, 0.25), aspect: 'motion' },
    knob('shape', 'Shape', 0, WAVES.length - 1, 0, 0),
    knob('offset', 'Offset', 0, 1, 0, 0),
    knob('ratio', 'Ratio', 0, RATIOS.length - 1, UNIT_RATIO, UNIT_RATIO),
    AMOUNT,
  ],
  undefined,
  () => {
    let u = 0;
    // Clocked: seconds since the last tick, the last two's period (0: not known yet), the share of it gone as last
    // given, whether a tick has come since it was plugged in, and whether each trigger is still high from its last
    // firing (a trigger counts rising past RISE, as the counter's).
    let since = 0;
    let period = 0;
    let between = 0;
    let seen = false;
    let clockHigh = false;
    let resetHigh = false;
    return (dt, _signals, k) => {
      if (Number.isFinite(k.clock)) {
        since += dt;
        const reset = !resetHigh && k.reset > RISE;
        if (reset) resetHigh = true;
        else if (resetHigh && !(k.reset >= RESET)) resetHigh = false;
        const tick = !clockHigh && k.clock > RISE;
        if (tick) clockHigh = true;
        else if (clockHigh && !(k.clock >= RESET)) clockHigh = false;
        // How far on it goes this frame, in ticks.
        let advance: number;
        if (tick) {
          advance = seen ? 1 - between : 0;
          if (seen) period = since;
          seen = true;
          since = 0;
          between = 0;
        } else {
          const now = period > 0 ? Math.min(1, since / period) : between;
          advance = now - between;
          between = now;
        }
        u = reset ? 0 : frac(u + advance * ratioOf(k.ratio));
        if (reset) {
          since = 0;
          between = 0;
        }
      } else {
        seen = false;
        between = 0;
        clockHigh = false;
        resetHigh = false;
        u = frac(u + k.rate * dt);
      }
      const p = u + k.offset;
      // Between two waves: Shape's share of the way from one to the next (the first two as the LFO always had them).
      const i = k.shape <= 1 ? 0 : Math.min(WAVES.length - 2, Math.ceil(k.shape) - 1);
      const a = WAVES[i](p);
      return k.amount * (a + (WAVES[i + 1](p) - a) * (k.shape - i));
    };
  },
  ['clock', 'reset'],
);

/** A number −1..1 for each whole step of a walk, the same for the same seed (an integer hash); the Grid's jitter too. */
export function point(seed: number, i: number): number {
  let h = Math.imul(i ^ Math.imul(seed, 0x9e3779b9), 0x85ebca6b);
  h ^= h >>> 13;
  h = Math.imul(h, 0xc2b2ae35);
  h ^= h >>> 16;
  return ((h >>> 0) / 4294967295) * 2 - 1;
}

// Random (VCV's, Vital's and Bitwig's name; the owner, 2026-10-20: Drift renamed): Rate new random points a second
// through −1..1, and Shape how it goes from one to the next: at 0 it glides the whole way (smootherstep, so it never
// jerks: what Drift was), at 1 it jumps (stepped, a sample-and-hold of random values), in between it glides over the
// first 1 − Shape of each step and holds (VCV's Random's blend). The seed is a hash of the module's id (the caller's),
// so two differ and a sync group's devices agree.
const random = define('random', 'Random', 'bipolar', [{ ...knob('rate', 'Rate', 0, 2, 0, 0.1), aspect: 'motion' }, knob('shape', 'Shape', 0, 1, 0, 0), AMOUNT], undefined, (seed) => {
  let u = 0;
  return (dt, _signals, k) => {
    u += k.rate * dt;
    const i = Math.floor(u);
    const glide = 1 - clamp(k.shape, 0, 1);
    const t = glide > 0 ? Math.min(1, (u - i) / glide) : 1;
    const s = t * t * t * (t * (t * 6 - 15) + 10);
    const a = point(seed, i);
    return k.amount * (a + (point(seed, i + 1) - a) * s);
  };
});

// Sample-and-hold: takes its input Rate times a second and holds it, gliding to each new value over Glide.
const hold = define(
  'hold',
  'Sample and hold',
  'unipolar',
  [{ ...knob('rate', 'Rate', 0, 16, 0, 2), aspect: 'motion' }, knob('glide', 'Glide', 0, 2, 0, 0), FULL, AMOUNT],
  IN,
  () => {
    let phase = 0;
    let held = 0;
    let y = 0;
    return (dt, _signals, k) => {
      // A hair under 1 counts: sums of rate × dt fall just short of a whole sample.
      phase += k.rate * dt;
      if (phase >= 1 - 1e-9) {
        phase = Math.max(0, phase - Math.floor(phase + 1e-9));
        held = k.in;
      }
      y = held + (y - held) * keep(dt, k.glide);
      return k.amount * unit(y, k.full);
    };
  },
);

/** Every modulator, by its type. */
export const MODULATORS: Record<string, ModulatorType> = Object.fromEntries(
  [volume, fast, envelope, average, peak, mean, gate, counter, lfo, random, hold].map((t) => [t.type, t]),
);

/** Types saved under an old name (v1 patches, until toV2 converts them): Drift is Random at Shape 0; the Slow follower
 *  an Envelope Follower. */
export const RENAMED: Readonly<Record<string, string>> = { drift: 'random', slow: 'envelope' };
/** A modulator by its type, or by a name it had. */
export const modulatorOf = (type: string): ModulatorType | undefined => MODULATORS[type] ?? MODULATORS[RENAMED[type]];

/** Each knob at its neutral, where the modulator gives 0. */
export const neutralOf = (t: ModulatorType) => Object.fromEntries(t.knobs.map((k) => [k.id, k.neutral]));
/** Each knob at its default. */
export const defaultsOf = (t: ModulatorType) => Object.fromEntries(t.knobs.map((k) => [k.id, k.default]));
