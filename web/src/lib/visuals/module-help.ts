// What each module type (decision 0057) and each of its ports does, a line each: for the graph's node and the long-hold
// help (held.svelte.ts), never shown otherwise. A knob that already has its text in help.ts (a setting's, a band's
// time, a Music signal's) points to it rather than saying it again; only what help.ts lacks is written here. A port
// whose label says it all has no line (PLAIN). Every declared module type and port has one or the other: tests/
// module-help.test.ts holds a new module to that. Pure.

import { HELP } from './help';
import { RENAMED } from './modulators/modulators';
import { SETTING_KNOB } from './patch';
import { SHOW_PARTS } from './ports';
import { BAND_SIGNALS, TRACK_SIGNALS } from './routes';

/** What a module does to the picture or the values, a line each, by its type. */
export const moduleHelp: Record<string, string> = {
  // Sources.
  flame: 'The fractal flame: six parts, one for each band of the sound, drawn as light.',
  shapes: 'Polygons on a ring, outlined or filled, each spinning as the ring turns.',
  waves: 'The sound’s waveform drawn as a line, a circle or spokes (without live sound, the bands’ levels).',

  // Effects.
  show: 'The flame’s output stage: its light turned into a picture, with size, glow, saturation, smoothing, haze and waves.',
  tonemap: 'Tone map: turns the flame’s light into a picture, how bright set by Glow.',
  curve: 'A tone curve: lifts the faint areas into view or sinks them to black.',
  saturate: 'Makes the colours richer or paler.',
  blur: 'Softens sparse, grainy areas into a blur; dense areas stay sharp.',
  bend: 'Bends the picture along slow giant waves.',
  fold: 'Folds the picture by a symmetry: mosaic, kaleidoscope, mirror or pixelate.',
  instance: 'Draws its picture once at each point.',
  'kaleidoscope-engine': 'One of the Kaleidoscope’s effects over the picture: Kaleid, Repeat, Rotate, Scroll, Pixelate or Magnify.',
  'eyedo-engine': 'One of EYEDO’s effects over the picture: Pulse, Burst, Halo, Rings or Interference.',
  'hydra-engine': 'One of Hydra’s effects over the picture: Echo, Ripple, Stream, Colorama, Hue, Saturate or Posterize.',
  warp: 'Moves the last frame’s picture before it is kept: zoom, turn, drift, stretch and wobble.',
  keep: 'Keeps part of the last frame’s picture, so movement leaves trails (MilkDrop’s decay).',
  'fluid-loop': 'Carries the picture along a fluid flow the music stirs, like smoke.',
  fluid: 'Makes the flame’s light flow like smoke, stirred by the music.',
  tunnel: 'Streams the flame’s light outward from the middle for ever, as if flying into it.',
  dive: 'Zooms for ever into a small, turned copy of the flame inside the flame.',
  trails: 'How long the flame’s light lingers behind it as it moves.',
  'loop-effect': 'An effect on the flame’s light inside its own loop, so it builds up into new shapes.',

  // Mixers.
  mix: 'Mixes up to four pictures, each by its blend mode and amount, the first under the rest.',
  feedback: 'The picture its input had last frame: the only way to close a picture loop.',

  // Modulators.
  track: 'A pulse as each new track starts, and the track’s seed, which sets its own look.',
  envelope: 'An envelope follower on the loudness; it scales every cable’s depth unless unplugged.',
  clock: 'The tempo’s clock: a turn every so many beats, and beat, bar and phrase triggers.',
  accent: 'Splits a value into a slow course and the swing about it, and keeps Accent’s share of the swing.',
  vca: 'A VCA: its input times its gain, the one way one signal multiplies another.',
  merge: 'Makes one channel of a polyphonic cable’s several (VCV’s poly to mono).',
  compressor: 'A sidechain compressor on values: turns its input down as the sidechain goes over the threshold.',
  adsr: 'An ADSR envelope on each gate or trigger, one a channel.',
  sequencer: 'A step sequencer: up to sixteen values, one a clock step.',
  looper: 'Records a value over a number of bars and loops it in time with the Clock.',
  grid: 'Points on a lattice, with each point’s values as channels.',
  lfo: 'A low-frequency oscillator, sine to triangle; in time with a clock when one is plugged in.',
  random: 'Random values at a rate, from smooth glides to stepped jumps.',
  hold: 'Sample and hold: takes its input at a rate and holds it.',
  volume: 'The bands’ levels added up, each by its weight (MilkDrop’s vol).',
  fast: 'An envelope follower with a short attack and release: follows its input closely.',
  slow: 'An envelope follower with a long attack and release: follows its input’s broad changes.',
  average: 'A moving average of its input over its window.',
  peak: 'Peak hold: the highest input lately, falling back over its Fall.',
  mean: 'The running mean of its input over its horizon, from its first value on.',
  gate: 'A comparator with hysteresis: gives 1 while its input is above the threshold.',
  counter: 'Counts each time its input rises past half way, stepping by 1/Length and wrapping at 1.',

  // In / Out.
  music: 'The playing music: its sound, the whole track’s measures and each band’s, as values and triggers.',
  filterbank: 'A crossover: splits the sound into six bands, with each band’s level.',
  meter: 'A detector: how busy a picture is, as motion, churn, zoom pumping and new looks.',
  screen: 'Draws what reaches it, through the flash guard.',
  frame: 'The settings that act on the whole picture, and the device’s own.',

  // Not a declared type: a group of modules (0057 §6).
  device: 'A group of modules shown as one, with its macro knobs; open it to see and change what is inside.',
};

// Lines several types share.
const AMOUNT_EFFECT = 'How much of the effect shows: none at 0, all of it at 1.';
const AMOUNT_OUT = 'Scales what it gives: nothing at 0, all of it at 1.';
const FULL = 'The input that gives a full output (1 is typical): higher needs a stronger signal.';
const LIGHT = 'The flame’s light, before Tone map turns it into a picture.';
const HUE = 'The colour, round the colour wheel.';
const SATURATION = 'How vivid the colours are: grey at 0.';
const TINT = 'How much it takes the flame’s colours around it.';
const GLOW = 'How much soft light spreads around it.';
const ACROSS = 'Moves it left or right.';
const UP = 'Moves it up or down.';
const SPIN = 'How fast it turns; below 0 the other way.';
const ATTACK = 'How quickly it rises to a higher input (seconds).';
const RELEASE = 'How quickly it falls to a lower input (seconds).';
/** Lines for the inputs every level knob and every step knob has (ports.ts): `<knob>:gain`, `<knob>:next`. */
const SUFFIX: Record<string, string> = {
  gain: 'Its knob times this, from 0 to 1: a VCA’s output cabled in; 1 while unplugged.',
  next: 'A trigger here moves its knob on to the next step, at its module’s neutral.',
};

const numbered = (n: number, id: (i: number) => string, line: (i: number) => string) => Object.fromEntries(Array.from({ length: n }, (_, i) => [id(i + 1), line(i + 1)]));

/** A port's or knob's own line, where its label does not say it all and help.ts has none. */
export const portHelp: Record<string, Record<string, string>> = {
  flame: { light: 'Its light: in, as it was last frame; out, as drawn now, before Tone map.' },
  shapes: {
    count: 'How many shapes on the ring; none at 0.',
    sides: 'Corners a shape; between whole numbers it morphs, near a circle at many.',
    radius: 'How big each shape is.',
    thickness: 'How wide the outline is.',
    fill: 'From an outline (0) to filled (1).',
    spread: 'How far out the ring is drawn.',
    x: ACROSS,
    y: UP,
    orbit: 'How fast the shapes travel round the ring; below 0 the other way.',
    rotation: 'Turns every shape.',
    spin: SPIN,
    twist: 'Turns each shape a little more than the one before.',
    taper: 'Each shape smaller (above 0) or larger (below 0) than the one before.',
    hue: HUE,
    hueSpread: 'How far the colour moves round the wheel from shape to shape.',
    saturation: SATURATION,
    additive: 'From covering what is under it (0) to adding its light to it (1).',
    tint: TINT,
    glow: GLOW,
  },
  waves: {
    width: 'How wide the line is; none at 0.',
    amplitude: 'How far the wave swings from its line.',
    circle: 'From a line (0) to a circle (1).',
    radial: 'Draws it as spokes out from the circle (1).',
    radius: 'How big the circle is.',
    length: 'How much of the screen’s width the line spans.',
    x: ACROSS,
    y: UP,
    rotation: 'Turns the wave.',
    spin: SPIN,
    smoothing: 'Smooths the line: jagged at 0, rounded at 1.',
    hue: HUE,
    hueSpread: 'How far the colour moves round the wheel along the line.',
    saturation: SATURATION,
    tint: TINT,
    glow: GLOW,
  },
  tonemap: { light: LIGHT },
  fluid: { light: LIGHT },
  tunnel: { light: LIGHT },
  dive: { light: LIGHT },
  trails: { light: LIGHT },
  'loop-effect': { light: LIGHT },
  fold: {
    grid: 'A Grid’s points set the symmetry; without one, its own centre and Count.',
    mode: 'Mosaic, kaleidoscope, mirror or pixelate.',
    count: 'How many times it repeats.',
    turn: 'Turns the fold; below 0 the other way.',
    amount: AMOUNT_EFFECT,
  },
  instance: { points: 'Where to draw it: a Grid’s points.', picture: 'What to draw at each point.', amount: AMOUNT_EFFECT },
  warp: {
    amount: AMOUNT_EFFECT,
    zoom: 'Zooms the kept picture in (above 0) or out (below 0), a second.',
    turn: 'Turns the kept picture, a second; below 0 the other way.',
    driftX: 'Moves the kept picture sideways, a second.',
    driftY: 'Moves the kept picture up or down, a second.',
    stretch: 'Stretches the kept picture wider (above 0) or taller (below 0), a second.',
    wobble: 'Bends the kept picture as water does.',
    wobbleSize: 'How fine the wobble’s ripples are.',
  },
  keep: {
    amount: 'How much of the last picture stays: none at 0, as without a loop.',
    trails: 'How long the trails last: a tenth of a second at 0, six seconds at 1.',
  },
  'fluid-loop': {
    amount: 'How far the picture moves with the flow: not at all at 0.',
    stir: 'How hard the music pushes the flow.',
    flow: 'How long the flow’s motion lasts.',
    swirl: 'How strongly its small whirls are kept.',
  },
  mix: {
    in1: 'The bottom picture; the others go over it in order.',
    ...numbered(4, (i) => `mode${i}`, (i) => `How picture ${i} blends: add, screen, over, multiply, window or lighten.`),
    ...numbered(4, (i) => `amount${i}`, (i) => `How much of picture ${i} shows.`),
  },
  feedback: { picture: 'In: the picture to hold back a frame; out: that picture as it was last frame.' },
  track: { pulse: 'A pulse as each new track starts.', seed: 'The track’s own number: the same track, the same look.' },
  envelope: {
    in: 'What it follows: Music’s loudness unless another cable is plugged in.',
    time: 'How quickly it follows (seconds).',
    level: 'The loudness, as far as Intensity lets it count.',
    out: 'The level while the music plays, else 0: every cable’s depth unless unplugged.',
  },
  clock: {
    tempo: 'Music’s tempo unless another cable is plugged in.',
    beats: 'Beats a turn: how many beats it takes to go once round.',
    phase: 'Where it is in its turn, from 0 to 1.',
    rate: 'How fast it turns now.',
  },
  accent: {
    in: 'The value to split.',
    accent: 'How much of the swing about the course it keeps: none at 0, twice it at 2.',
    course: 'How slowly the course follows (seconds): the swing about it is what Accent keeps.',
  },
  vca: { gain: 'The gain: 0 shuts the input off, 1 lets it through as it is, 2 doubles it.', cv: 'The gain as a signal (CV), cabled in.' },
  merge: { mode: 'Sum, mean, largest or geometric mean of the channels.' },
  compressor: {
    in: 'The signal it compresses.',
    key: 'The sidechain: the signal whose level sets how far it compresses.',
    threshold: 'The sidechain level above which it starts to compress.',
    ratio: 'How hard it compresses above the threshold: at 4, a quarter of the excess gets through.',
    knee: 'How gently it starts around the threshold: a hard knee at 0.',
    attack: 'How quickly it clamps down (seconds).',
    release: 'How quickly it lets go (seconds).',
    makeup: 'Makeup gain, after compressing.',
    out: 'The compressed signal.',
    reduction: 'The gain reduction (1: none), to duck anything else.',
  },
  adsr: {
    gate: 'Starts an envelope on each trigger, one a channel.',
    attack: 'Attack: the time to rise to the peak (seconds).',
    decay: 'Decay: the time to fall from the peak to Sustain (seconds).',
    sustain: 'Sustain: the level held while the gate stays open.',
    release: 'Release: the time to fall to 0 once the gate closes (seconds).',
    retrigger: 'A trigger while one rings starts from the higher of the two (0) or from where it is (1).',
    amount: 'The peak’s height.',
    out: 'One envelope a channel.',
  },
  sequencer: {
    clock: 'Moves on a step; the Clock’s beat unless unplugged.',
    reset: 'Back to the first step; on the Clock’s bar unless unplugged.',
    length: 'How many steps before it starts over.',
    out: 'The value of the step it is on.',
    gate: 'A trigger on each step.',
  },
  looper: {
    in: 'The value it records.',
    clock: 'Keeps the loop in time; the Clock’s bar unless unplugged.',
    bars: 'How many bars the loop lasts.',
    record: 'On: records its input over the loop.',
    overdub: 'On: adds its input to what is recorded.',
    out: 'The loop as recorded.',
  },
  grid: {
    lattice: 'Square, hexagonal, triangular, rings or spiral.',
    count: 'How many points.',
    spacing: 'How far apart the points are.',
    rotation: 'Turns the whole grid.',
    jitter: 'Moves each point off its place at random.',
    points: 'The points, for Fold or Instance.',
    index: 'Each point’s number, from 0 to 1.',
    distance: 'Each point’s distance from the centre.',
    angle: 'Each point’s angle round the centre.',
  },
  lfo: {
    clock: 'Plugged in, Rate counts in beats instead of seconds.',
    reset: 'Starts the wave again; on the Clock’s bar unless unplugged.',
    rate: 'Cycles a second, or a beat while a clock is plugged in.',
    ratio: 'With a clock plugged in, the clock divided or multiplied (÷4 to ×4).',
    shape: 'From a sine (0) to a triangle (1).',
    offset: 'Where in its cycle it starts.',
    amount: AMOUNT_OUT,
  },
  random: { rate: 'New random values a second.', shape: 'From smooth glides (0) to stepped jumps (1).', amount: AMOUNT_OUT },
  hold: { rate: 'Samples a second.', glide: 'How long it glides to each new value (seconds); a jump at 0.', full: FULL, amount: AMOUNT_OUT },
  volume: { ...numbered(6, (i) => `band${i}`, () => 'This band’s weight in the sum; left out at 0.'), full: FULL, amount: AMOUNT_OUT },
  fast: { attack: ATTACK, release: RELEASE, full: FULL, amount: AMOUNT_OUT },
  slow: { attack: ATTACK, release: RELEASE, full: FULL, amount: AMOUNT_OUT },
  average: { window: 'About how many seconds it averages over.', full: FULL, amount: AMOUNT_OUT },
  peak: { fall: 'How quickly the held peak falls back (seconds).', full: FULL, amount: AMOUNT_OUT },
  mean: { horizon: 'About how many seconds back it remembers.', full: FULL, amount: AMOUNT_OUT },
  gate: {
    above: 'The threshold it opens above (1: above typical).',
    hysteresis: 'How far under the threshold it must fall to close, as a share.',
    lift: 'How much each opening raises the next one’s threshold.',
    recover: 'How quickly that raised threshold falls back (seconds).',
    release: 'How quickly its output falls once it closes (seconds).',
    gap: 'The least time between two openings (seconds; 0: none).',
    amount: AMOUNT_OUT,
  },
  counter: { length: 'Steps to a full count: at 4, beats into it count a bar.', amount: AMOUNT_OUT },
  music: { audio: 'The sound itself, its recent samples.', playing: '1 while the music plays, 0 while it is paused.', hold: 'How long each band’s envelope takes to fall (seconds).' },
  filterbank: {
    audio: 'The sound to split: Music’s unless unplugged.',
    ...numbered(5, (i) => `split${i}`, (i) => `Where band ${i} ends and band ${i + 1} begins (Hz).`),
    bands: 'The six bands’ sound, one a channel.',
    level: 'Each band’s level, one a channel.',
  },
  meter: {
    motion: 'How much the picture moves.',
    shape: 'How quickly its shapes change.',
    colour: 'How quickly its colours change.',
    zoom: 'How much its size pumps.',
    looks: 'How many new looks a minute.',
  },
  screen: { guard: 'How far the flash guard is holding back now (at most three flashes a second over a large area).' },
  frame: { loose: 'Set by a double tap of the dice: up to twice Wild’s limits until the next roll, an undo or a reset.' },
};

/** Ports whose label says all there is (no line): on any type, and on these. */
const PLAIN_ANYWHERE: ReadonlySet<string> = new Set(['picture', 'in', 'out']);
export const PLAIN: Record<string, readonly string[]> = {
  shapes: ['opacity'],
  waves: ['opacity'],
  clock: ['beat', 'bar', 'phrase'],
  grid: ['row', 'column'],
  mix: ['in2', 'in3', 'in4'],
  sequencer: Array.from({ length: 16 }, (_, i) => `step${i + 1}`),
};

/** The module a setting's knob is on (patch.ts SETTING_KNOB) as its type: the effect slots are their engines. */
const SLOT_TYPE: Record<string, string> = { fx1: 'kaleidoscope-engine', fx2: 'eyedo-engine', fx3: 'hydra-engine' };

/** help.ts's key for a knob or port that has its text there, by type and id. */
export const HELP_KEY: Record<string, Record<string, string>> = (() => {
  const out: Record<string, Record<string, string>> = {};
  const add = (type: string, id: string, key: string) => ((out[type] ??= {})[id] = key);
  // A setting's knob: its setting's text, where it has one.
  for (const [setting, [module, id]] of Object.entries(SETTING_KNOB)) if (HELP[setting]) add(SLOT_TYPE[module] ?? module, id, setting);
  // Show's parts took their knob from Show, id and all.
  for (const part of SHOW_PARTS) if (out.show?.[part.knob]) add(part.type, part.knob, out.show[part.knob]);
  // The flame's per-band times, the walks' cable ends, and the knobs the whole frame's settings are.
  for (const id of ['lighting', 'evolution', 'continuity']) add('flame', id, id);
  add('frame', 'lookWalk', 'aspect:look');
  add('frame', 'behaviourWalk', 'aspect:behaviour');
  add('envelope', 'intensity', 'intensity');
  add('clock', 'speed', 'speed');
  // Music's outputs: the signals' texts.
  for (const s of [...TRACK_SIGNALS, ...BAND_SIGNALS]) add('music', s, `signal:${s}`);
  return out;
})();

/** Types the engine has renamed (0057 §7e: Flash is Accent). */
const WAS: Record<string, string> = { flash: 'accent' };

/** Whether a port's label says all (no line wanted). */
export const isPlain = (type: string, id: string) => PLAIN_ANYWHERE.has(id) || (PLAIN[type]?.includes(id) ?? false);

/** The help for a module type, or for one of its ports or knobs (a polyphonic knob's channel, `lighting.3`, as its
 *  knob; a knob's `:gain` or `:next` input, the line all of those share): its own line, else help.ts's text for it;
 *  undefined where the label says it all. A type saved under an old name (`drift`, `flash`) is read as the one it is now. */
export function helpFor(type: string, portOrKnob?: string): string | undefined {
  const t = RENAMED[type] ?? WAS[type] ?? type;
  if (portOrKnob === undefined) return moduleHelp[t];
  const id = portOrKnob.replace(/\.\d+$/, '');
  const suffix = /:(\w+)$/.exec(id)?.[1];
  if (suffix && SUFFIX[suffix]) return SUFFIX[suffix];
  const own = portHelp[t]?.[id];
  if (own) return own;
  const key = HELP_KEY[t]?.[id];
  return key ? HELP[key] : undefined;
}
