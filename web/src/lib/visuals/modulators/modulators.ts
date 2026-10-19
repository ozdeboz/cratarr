// The visuals' modulators (decision 0056): modules with no picture, whose outputs feed cables as the music's signals do.
// The set is what curated MilkDrop presets build by hand before they drive anything: summed volume, fast and slow
// followers, moving averages, peak and mean trackers, "above typical" beat gates, beat and bar counters, LFOs and
// drift, sample-and-hold.
//
// - Each reads its input through `signals` (1 = typical) and gives 0..1 (`range: 'unipolar'`) or −1..1 ('bipolar').
// - Every knob is continuous, and with every knob at its neutral the output is 0 (Amount at 0, the rest at "does
//   nothing"), so a chain change fades a modulator in from its neutral or out to it.
// - Times are seconds and smoothing is exponential over dt, so 30 and 120 frames a second give the same.
//
// Pure: no WebGL, no DOM.

import type { Knob, Modulator, ModuleType, SignalRef } from '../patch';

/** The music's signals now, 1 = typical. */
export type Signals = (ref: SignalRef) => number;

/** A modulator's module type: what it gives (0..1 or −1..1), the signal it reads unless the chain names another, and a
 *  new running one. `seed` sets the way the wandering ones (drift) walk, so the devices of a sync group walk alike. */
export type ModulatorType = ModuleType & {
  place: 'modulator';
  range: 'unipolar' | 'bipolar';
  input?: SignalRef;
  create(input?: SignalRef, seed?: number): Modulator;
};

/** The bands a track's signals have (signals.ts). */
export const BANDS = 6;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
/** What is left of a gap after `dt` of an exponential approach with time constant `tau` (nothing at 0). */
const keep = (dt: number, tau: number) => (tau > 0 ? Math.exp(-dt / tau) : 0);
const frac = (u: number) => u - Math.floor(u);

/** A signal as read: never negative, 0 where there is none. */
function read(signals: Signals, ref: SignalRef): number {
  const v = signals(ref);
  return Number.isFinite(v) ? Math.max(0, v) : 0;
}

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
/** The signal that reads as 1 (at 2, typical is half way). */
const FULL = knob('full', 'Full at', 0.25, 4, 1, 2);
const unit = (x: number, full: number) => clamp(x / full, 0, 1);

/** Each knob as set, within its range; its default where it is not set. */
function settled(knobs: Knob[], set: Record<string, number>, into: Record<string, number>) {
  for (const k of knobs) {
    const v = set[k.id];
    into[k.id] = Number.isFinite(v) ? clamp(v, k.min, k.max) : k.default;
  }
  return into;
}

type Step = (dt: number, signals: Signals, k: Record<string, number>) => number;

function define(
  type: string,
  label: string,
  range: ModulatorType['range'],
  knobs: Knob[],
  input: SignalRef | undefined,
  make: (input: SignalRef, seed: number) => Step,
): ModulatorType {
  const [lo, hi] = range === 'bipolar' ? [-1, 1] : [0, 1];
  return {
    type,
    label,
    place: 'modulator',
    range,
    knobs,
    input,
    create(from = input ?? { signal: 'level' }, seed = 0): Modulator {
      const step = make(from, seed);
      const k: Record<string, number> = {};
      return {
        step(dt, signals, set) {
          const v = step(Number.isFinite(dt) ? Math.max(0, dt) : 0, signals, settled(knobs, set, k));
          return Number.isFinite(v) ? clamp(v, lo, hi) : 0;
        },
      };
    },
  };
}

const BASS: SignalRef = { signal: 'level', band: 0 };

// MilkDrop's vol: the bands' levels together, each by its weight (their mean once the weights reach 1, so all six at 1
// read as typical at 1).
const volume = define(
  'volume',
  'Volume',
  'unipolar',
  [...Array.from({ length: BANDS }, (_, b) => knob(`band${b + 1}`, `Band ${b + 1}`, 0, 1, 0, 1, 'level')), FULL, AMOUNT],
  { signal: 'level' },
  (input) => {
    const refs = Array.from({ length: BANDS }, (_, band) => ({ ...input, band }));
    return (_dt, signals, k) => {
      let sum = 0;
      let weight = 0;
      for (let b = 0; b < BANDS; b++) {
        const w = k[`band${b + 1}`];
        if (w > 0) {
          sum += w * read(signals, refs[b]);
          weight += w;
        }
      }
      return k.amount * unit(sum / Math.max(1, weight), k.full);
    };
  },
);

// A follower: rises with the attack's time constant, falls with the release's.
const follower = (type: string, label: string, attack: number, release: number) =>
  define(type, label, 'unipolar', [knob('attack', 'Attack', 0, 2, 0, attack), knob('release', 'Release', 0, 5, 0, release), FULL, AMOUNT], BASS, (input) => {
    let y = 0;
    return (dt, signals, k) => {
      const x = read(signals, input);
      y = x + (y - x) * keep(dt, x > y ? k.attack : k.release);
      return k.amount * unit(y, k.full);
    };
  });
const fast = follower('fast', 'Fast follower', 0.01, 0.15);
const slow = follower('slow', 'Slow follower', 0.4, 1.5);

// A moving average over about its window, from silence.
const average = define('average', 'Average', 'unipolar', [knob('window', 'Window', 0, 8, 0, 1), FULL, AMOUNT], BASS, (input) => {
  let y = 0;
  return (dt, signals, k) => {
    const x = read(signals, input);
    y = x + (y - x) * keep(dt, k.window);
    return k.amount * unit(y, k.full);
  };
});

// The peak: the highest lately, falling toward 0 with its time constant and caught again by anything above it.
const peak = define('peak', 'Peak', 'unipolar', [knob('fall', 'Fall', 0, 8, 0, 1.5), FULL, AMOUNT], BASS, (input) => {
  let y = 0;
  return (dt, signals, k) => {
    y = Math.max(read(signals, input), y * keep(dt, k.fall));
    return k.amount * unit(y, k.full);
  };
});

// The mean of what has been heard, forgetting what is older than about its horizon. Unlike Average it starts from the
// first values, not from silence (the weight heard so far divides it out).
const mean = define('mean', 'Mean', 'unipolar', [knob('horizon', 'Horizon', 0, 60, 0, 10), FULL, AMOUNT], BASS, (input) => {
  let m = 0;
  let w = 0;
  return (dt, signals, k) => {
    const x = read(signals, input);
    const left = keep(dt, k.horizon);
    m = x + (m - x) * left;
    w = 1 + (w - 1) * left;
    return k.amount * unit(w > 0 ? m / w : x, k.full);
  };
});

// The beat gate: opens when the signal goes above typical times Above (MilkDrop's beat tests sit at 1), closes below
// that less its Hysteresis share. Each opening lifts the next one's threshold by Lift, which fades over Recover, as
// presets' adaptive thresholds do. Open it gives 1; closed it falls over Release.
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
    AMOUNT,
  ],
  BASS,
  (input) => {
    let open = false;
    let lifted = 0;
    let y = 0;
    return (dt, signals, k) => {
      const x = read(signals, input);
      lifted *= keep(dt, k.recover);
      if (!open && x > k.above + lifted) {
        open = true;
        lifted += k.lift;
      } else if (open && x < k.above * (1 - k.hysteresis)) open = false;
      y = open ? 1 : y * keep(dt, k.release);
      return k.amount * y;
    };
  },
);

// A counter: each time its signal rises above Above it moves on by 1/Length and wraps at 1 (0, ¼, ½, ¾, 0 … at 4).
// Length moves the step, not the count, so it morphs without a jump; at 1 the count stays at 0.
const counter = (type: string, label: string, signal: string) =>
  define(
    type,
    label,
    'unipolar',
    [knob('length', 'Length', 1, 16, 1, 4), knob('above', 'Above', 0.25, 3, 1, 1), AMOUNT],
    { signal },
    (input) => {
      let high = false;
      let phase = 0;
      return (_dt, signals, k) => {
        const x = read(signals, input);
        if (!high && x > k.above) {
          high = true;
          phase = frac(phase + 1 / k.length);
          if (1 - phase < 1e-9) phase = 0;
        } else if (high && x < k.above * 0.8) high = false;
        return k.amount * phase;
      };
    },
  );
const beats = counter('beats', 'Beat counter', 'beat');
const bars = counter('bars', 'Bar counter', 'downbeat');

// An LFO: Rate cycles a second, from a sine (Shape 0) to a triangle (1), started Offset of a cycle in.
const lfo = define(
  'lfo',
  'LFO',
  'bipolar',
  [knob('rate', 'Rate', 0, 8, 0, 0.25), knob('shape', 'Shape', 0, 1, 0, 0), knob('offset', 'Offset', 0, 1, 0, 0), AMOUNT],
  undefined,
  () => {
    let u = 0;
    return (dt, _signals, k) => {
      u = frac(u + k.rate * dt);
      const p = u + k.offset;
      const sine = Math.sin(2 * Math.PI * p);
      const triangle = 4 * Math.abs(frac(p - 0.25) - 0.5) - 1;
      return k.amount * (sine + (triangle - sine) * k.shape);
    };
  },
);

/** A number −1..1 for each whole step of a walk, the same for the same seed (an integer hash). */
function point(seed: number, i: number): number {
  let h = Math.imul(i ^ Math.imul(seed, 0x9e3779b9), 0x85ebca6b);
  h ^= h >>> 13;
  h = Math.imul(h, 0xc2b2ae35);
  h ^= h >>> 16;
  return ((h >>> 0) / 4294967295) * 2 - 1;
}

// Drift: a smooth wander through −1..1, Rate new points a second, eased between them (smootherstep, so it never
// jerks). The same seed walks the same way.
const drift = define('drift', 'Drift', 'bipolar', [knob('rate', 'Rate', 0, 2, 0, 0.1), AMOUNT], undefined, (_input, seed) => {
  let u = 0;
  return (dt, _signals, k) => {
    u += k.rate * dt;
    const i = Math.floor(u);
    const t = u - i;
    const s = t * t * t * (t * (t * 6 - 15) + 10);
    const a = point(seed, i);
    return k.amount * (a + (point(seed, i + 1) - a) * s);
  };
});

// Sample-and-hold: takes the signal Rate times a second and holds it, gliding to each new value over Glide.
const hold = define(
  'hold',
  'Sample and hold',
  'unipolar',
  [knob('rate', 'Rate', 0, 16, 0, 2), knob('glide', 'Glide', 0, 2, 0, 0), FULL, AMOUNT],
  BASS,
  (input) => {
    let phase = 0;
    let held = 0;
    let y = 0;
    return (dt, signals, k) => {
      // A hair under 1 counts: sums of rate × dt fall just short of a whole sample.
      phase += k.rate * dt;
      if (phase >= 1 - 1e-9) {
        phase = Math.max(0, phase - Math.floor(phase + 1e-9));
        held = read(signals, input);
      }
      y = held + (y - held) * keep(dt, k.glide);
      return k.amount * unit(y, k.full);
    };
  },
);

/** Every modulator, by its type. */
export const MODULATORS: Record<string, ModulatorType> = Object.fromEntries(
  [volume, fast, slow, average, peak, mean, gate, beats, bars, lfo, drift, hold].map((t) => [t.type, t]),
);

/** Each knob at its neutral, where the modulator gives 0. */
export const neutralOf = (t: ModulatorType) => Object.fromEntries(t.knobs.map((k) => [k.id, k.neutral]));
/** Each knob at its default. */
export const defaultsOf = (t: ModulatorType) => Object.fromEntries(t.knobs.map((k) => [k.id, k.default]));
