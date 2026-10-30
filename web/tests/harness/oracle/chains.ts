// FROZEN (0.10: the app reaches none of it; the tests' and scripts/check-migration's fixture): the visuals' preset chains (decision 0056: "chains are presets"). The owner, after 70: "we should also have some
// sensible presets, but nothing extreme the way we now have. Currently we have way too many lines. So we need a few
// sensible chains as presets, the ability to start from scratch"; asked which: Calm, Pulse and Flow; and from scratch,
// nothing at all (an empty patch: no source, so nothing is drawn until one is added).
//
// Each is a handful of cables (routes; anything not listed is off), mostly from the whole track's signals, not one a
// band (today's default chain has seven a band, 54 in all), and the few settings that make it what it is. The look
// (form, colours …) stays the track's own, as a preset leaves it.

import type { RouteValue } from '../../../src/lib/visuals/curves.ts';
import { emptyExtra, type Cable, type Module, type PatchExtra } from '../../../src/lib/visuals/patch.ts';
import { extraType } from '../../../src/lib/visuals/registry.ts';
import { defaultRoutes, routeKey } from '../../../src/lib/visuals/routes.ts';
import type { VisualSettings } from '../../../src/lib/visuals/settings.svelte.ts';

/** The settings a chain may set: how the frame behaves, the loop's and finish's levels, and whether the flame is drawn
 *  (`flameLayer`, its layer's amount, 0 from scratch); never the look. */
export type ChainSettings = Partial<
  Pick<
    VisualSettings,
    | 'flash'
    | 'speed'
    | 'smoothing'
    | 'fluid'
    | 'stir'
    | 'flow'
    | 'swirl'
    | 'waves'
    | 'trails'
    | 'tunnel'
    | 'dive'
    | 'effects'
    | 'facets'
    | 'fx1'
    | 'fx1K1'
    | 'fx2'
    | 'fx3'
    | 'fx3K1'
    | 'fx1K2'
    | 'fxLoop'
    | 'fxLoopK1'
    | 'fx1Amount'
    | 'fx2Amount'
    | 'fx3Amount'
    | 'fxLoopAmount'
  >
> & { flameLayer?: number };

export type Chain = { id: string; label: string; line: string; routes: Record<string, RouteValue>; extra: PatchExtra; settings?: ChainSettings };

/** None of the loop's and finish's own: what every chain starts from, so one chosen after another leaves none of it. */
const QUIET: ChainSettings = { flameLayer: 1, fx1: 0, fx2: 0, fx3: 0, fxLoop: 0, fluid: 0, tunnel: 0, dive: 0, effects: 0, facets: 0, waves: 0, fx1Amount: 0, fx2Amount: 0, fx3Amount: 0, fxLoopAmount: 0 };

// What the preset study taught (9,109 of Cream of the Crop's presets, read for how they are made, none of them copied;
// visuals-milkdrop-direction): a preset makes its own modulators first and drives through them, not the raw signals;
// its beat test sits at typical (our gate's Above 1); the music drives the drawing and the zoom, while turning and
// drifting go with time; decay and finish stay put; the look follows the slow inputs, only the hits the fast ones;
// most run the picture's own loop (warp and keep). Each chain below holds to that, in at most eight cables.

/** One of the extras' modules at its type's defaults, some set (`over`). */
function mod(type: string, id: string, over: Record<string, number> = {}): Module {
  const t = extraType(type);
  return { id, type, knobs: { ...Object.fromEntries((t?.knobs ?? []).map((k) => [k.id, k.default])), ...over } };
}
const cable = (id: string, from: Cable['from'], module: string, knob: string, value: number, mode: Cable['mode'] = 'swing'): Cable => ({ id, from, to: { module, knob }, value, mode });

export const CHAINS: readonly Chain[] = [
  {
    // An Envelope Follower, slow, on the loudness breathes the size and the colour's strength; the turning wanders with time; the
    // hue moves with the swells, slowly.
    id: 'calm',
    label: 'Calm',
    line: 'Breathes with the loudness; its colours drift with the swells.',
    routes: {},
    extra: {
      ...emptyExtra(),
      modulators: [mod('envelope', 'breath', { attack: 0.8, release: 2.5 }), mod('random', 'wander', { rate: 0.05, amount: 1 })],
      cables: [
        cable('calm-in', { signal: 'loudness' }, 'breath', 'in', 1),
        cable('calm-size', { modulator: 'breath' }, 'flame', 'zoom', 0.5),
        cable('calm-saturation', { modulator: 'breath' }, 'show', 'saturation', 0.6),
        cable('calm-turn', { modulator: 'wander' }, 'flame', 'rotation', 0.3),
        cable('calm-hue', { signal: 'swell', speed: 'slow' }, 'flame', 'hue', 0.15),
      ],
    },
    settings: { ...QUIET, flash: 0.3, speed: 0.6, smoothing: 0.6, trails: 1 },
  },
  {
    // The bass's hits through a beat gate (at typical, lifting after each) push the flame's size and glow and the
    // soft hexagons around it, which take the flame's colours and smear in a short trail; the sub's hits light the
    // flame's parts; one drift turns the flame and the hexagons together.
    id: 'pulse',
    label: 'Pulse',
    line: 'The beat and the bass: the flame and its hexagons pushed on every hit.',
    routes: {
      [routeKey('hits', 0, 'lights')]: 1,
    },
    extra: {
      ...emptyExtra(),
      layers: [
        {
          id: 'pulse-shapes',
          source: mod('shapes', 'pulse-shapes-source', { count: 6, sides: 6, radius: 0.16, spread: 0.55, fill: 0.35, thickness: 0.008, opacity: 0.7, orbit: 0.02 }),
          modules: [mod('glow', 'pulse-shapes-glow', { glow: 0.7 }), mod('tint', 'pulse-shapes-tint', { tint: 0.9 })],
          loop: [],
          blend: { mode: 'screen', amount: 0.8 },
        },
      ],
      loop: [mod('warp', 'pulse-warp', { zoom: 0.04 }), mod('keep', 'pulse-keep', { amount: 0.5, trails: 0.4 })],
      modulators: [mod('gate', 'kick', { above: 1, lift: 0.3, release: 0.15, amount: 1 }), mod('random', 'wander', { rate: 0.1, amount: 1 })],
      cables: [
        cable('pulse-kick-in', { signal: 'hits', band: 1 }, 'kick', 'in', 1),
        cable('pulse-size', { modulator: 'kick' }, 'flame', 'zoom', 0.3),
        cable('pulse-glow', { modulator: 'kick' }, 'show', 'glow', 0.4),
        cable('pulse-radius', { modulator: 'kick' }, 'pulse-shapes-source', 'radius', 0.25),
        cable('pulse-turn', { modulator: 'wander' }, 'flame', 'rotation', 0.3),
        cable('pulse-spin', { modulator: 'wander' }, 'pulse-shapes-source', 'spin', 0.3),
      ],
    },
    settings: { ...QUIET, flash: 1, speed: 1 },
  },
  {
    // Smoke: the fluid stirred by the beats and the sub, flowing with the loudness (slowly), whirled by the swells; the
    // picture's own loop keeps a trail of it, the bass's hits zooming it in, its turn wandering.
    id: 'flow',
    label: 'Flow',
    line: 'Smoke: the fluid stirred by the beats and the bass, whirled by the swells, little flashing.',
    routes: {
      [routeKey('beat', null, 'stir')]: 1,
      [routeKey('hits', 0, 'stir')]: 1,
      [routeKey('swell', null, 'swirl')]: 1,
    },
    extra: {
      ...emptyExtra(),
      loop: [mod('warp', 'flow-warp', { zoom: 0.05 }), mod('keep', 'flow-keep', { amount: 0.6, trails: 0.5 })],
      modulators: [mod('random', 'wander', { rate: 0.05, amount: 1 })],
      cables: [
        cable('flow-flow', { signal: 'loudness', speed: 'slow' }, 'fluid', 'flow', 1),
        cable('flow-zoom', { signal: 'hits', band: 1 }, 'flow-warp', 'zoom', 0.2),
        cable('flow-turn', { modulator: 'wander' }, 'flow-warp', 'turn', 0.3),
      ],
    },
    settings: { ...QUIET, fluid: 0.8, stir: 1, flow: 1, swirl: 1, flash: 0.3, speed: 0.8, trails: 1 },
  },
  {
    // MilkDrop's tunnel: the picture's own loop zooming in for ever and keeping its trail, the bass's hits (through a
    // gate at typical) kicking the zoom, the turn wandering; the flame's own Tunnel streaming outward with the
    // loudness, slowly.
    id: 'tunnel',
    label: 'Tunnel',
    line: 'Zooming in for ever, the bass kicking it, a trail behind.',
    routes: {},
    extra: {
      ...emptyExtra(),
      loop: [mod('warp', 'tunnel-warp', { zoom: 0.15 }), mod('keep', 'tunnel-keep', { amount: 0.75, trails: 0.6 })],
      modulators: [mod('gate', 'kick', { above: 1, lift: 0.3, release: 0.2, amount: 1 }), mod('envelope', 'breath'), mod('random', 'wander', { rate: 0.05, amount: 1 })],
      cables: [
        cable('tunnel-kick-in', { signal: 'hits', band: 1 }, 'kick', 'in', 1),
        cable('tunnel-zoom', { modulator: 'kick' }, 'tunnel-warp', 'zoom', 0.3),
        cable('tunnel-turn', { modulator: 'wander' }, 'tunnel-warp', 'turn', 0.4),
        cable('tunnel-breath-in', { signal: 'loudness' }, 'breath', 'in', 1),
        cable('tunnel-stream', { modulator: 'breath' }, 'tunnel', 'tunnel', 0.3, 'level'),
      ],
    },
    settings: { ...QUIET, tunnel: 0.5, flash: 0.5, speed: 0.8 },
  },
  {
    // MilkDrop's waveform: the sound itself drawn (the Waves source) over a dimmed flame, its height with the loudness
    // (a fast follower), its trail kept and slowly turned by the picture's own loop.
    id: 'scope',
    label: 'Scope',
    line: 'The sound itself drawn as a wave, a trail behind it.',
    routes: {},
    extra: {
      ...emptyExtra(),
      layers: [{ id: 'scope', source: mod('waves', 'scope-source', { width: 0.01, opacity: 0.9 }), modules: [mod('glow', 'scope-glow', { glow: 0.7 }), mod('tint', 'scope-tint', { tint: 0.85 })], loop: [], blend: { mode: 'screen', amount: 1 } }],
      loop: [mod('warp', 'scope-warp', { zoom: 0.05 }), mod('keep', 'scope-keep', { amount: 0.7, trails: 0.5 })],
      modulators: [mod('fast', 'body'), mod('random', 'wander', { rate: 0.05, amount: 1 })],
      cables: [
        cable('scope-body-in', { signal: 'loudness' }, 'body', 'in', 1),
        cable('scope-height', { modulator: 'body' }, 'scope-source', 'amplitude', 0.4),
        cable('scope-turn', { modulator: 'wander' }, 'scope-warp', 'turn', 0.3),
      ],
    },
    settings: { ...QUIET, flameLayer: 0.7, flash: 0.3, speed: 0.6 },
  },
  {
    // Tiles: the Kaleidoscope engine's Repeat over the flame, how many stepping on with each bar (a counter of the
    // downbeats), Hydra's Posterize flattening the colours into cells; the size breathing slowly, the turn wandering.
    id: 'mosaic',
    label: 'Mosaic',
    line: 'The picture in tiles, their number moving on with each bar.',
    routes: {},
    extra: {
      ...emptyExtra(),
      modulators: [mod('counter', 'bars', { length: 4, amount: 1 }), mod('envelope', 'breath'), mod('random', 'wander', { rate: 0.05, amount: 1 })],
      cables: [
        cable('mosaic-bars-in', { signal: 'downbeat' }, 'bars', 'in', 1),
        cable('mosaic-tiles', { modulator: 'bars' }, 'fx1', 'k1', 0.6),
        cable('mosaic-breath-in', { signal: 'loudness' }, 'breath', 'in', 1),
        cable('mosaic-size', { modulator: 'breath' }, 'flame', 'zoom', 0.4),
        cable('mosaic-turn', { modulator: 'wander' }, 'flame', 'rotation', 0.3),
      ],
    },
    settings: { ...QUIET, effects: 1, fx1: 2, fx1K1: 0.3, fx1Amount: 1, fx3: 7, fx3K1: 0.6, fx3Amount: 0.6, flash: 0.4, speed: 0.7 },
  },
  {
    // Self-alike: the flame's Dive zooming into one part of it for ever, the picture's own loop turning and keeping
    // it as a spiral (MilkDrop's fractal feedback); the bass's hits push the dive, the turn wanders.
    id: 'fractal',
    label: 'Fractal',
    line: 'Diving into the flame for ever, a spiral of itself around it.',
    routes: {},
    extra: {
      ...emptyExtra(),
      // A ring of the sound in the flame's colours, drawn into the spiral and carried in by it.
      layers: [{ id: 'fractal-ring', source: mod('waves', 'fractal-ring-source', { circle: 1, radius: 0.3, width: 0.006, opacity: 0.8 }), modules: [mod('glow', 'fractal-ring-glow', { glow: 0.6 }), mod('tint', 'fractal-ring-tint', { tint: 0.95 })], loop: [], blend: { mode: 'screen', amount: 0.7 } }],
      loop: [mod('warp', 'fractal-warp', { zoom: 0.1, turn: 0.1 }), mod('keep', 'fractal-keep', { amount: 0.7, trails: 0.6 })],
      modulators: [mod('gate', 'kick', { above: 1, lift: 0.3, release: 0.3, amount: 1 }), mod('random', 'wander', { rate: 0.05, amount: 1 })],
      cables: [
        cable('fractal-kick-in', { signal: 'hits', band: 1 }, 'kick', 'in', 1),
        cable('fractal-dive', { modulator: 'kick' }, 'dive', 'dive', 0.3, 'level'),
        cable('fractal-turn', { modulator: 'wander' }, 'fractal-warp', 'turn', 0.4),
      ],
    },
    settings: { ...QUIET, dive: 0.5, flash: 0.4, speed: 0.7 },
  },
];

/** The smallest patches that each show one idea (the owner: "minimal example patches for basic concepts like different
 *  kaleidoscopic targets"). The study's first lesson about MilkDrop's pipeline: one effect means something else by where
 *  it stands: on one layer, on the whole picture, or inside the loop that feeds the picture back into itself. */
export const EXAMPLES: readonly Chain[] = [
  {
    // The Kaleidoscope engine's mirror on everything, last (the finish): the whole picture folded.
    id: 'example-kaleido-all',
    label: 'Kaleidoscope on everything',
    line: 'In the finish: the whole picture folded, last.',
    routes: {},
    extra: emptyExtra(),
    settings: { ...QUIET, effects: 1, fx1: 1, fx1K1: 0.4, fx1K2: 0.55, fx1Amount: 1 },
  },
  {
    // The same mirror as a module on one layer: only the shapes are folded, the flame beside them as it is.
    id: 'example-kaleido-layer',
    label: 'Kaleidoscope on one layer',
    line: 'On the shapes only: the flame behind them untouched.',
    routes: {},
    extra: {
      ...emptyExtra(),
      layers: [{ id: 'shapes', source: mod('shapes', 'shapes-source', { count: 6 }), modules: [mod('kaleidoscope-engine', 'shapes-kaleido', { effect: 1, amount: 1, k1: 0.4 })], loop: [], blend: { mode: 'screen', amount: 1 } }],
    },
    settings: { ...QUIET },
  },
  {
    // The mirror inside the flame's own loop: what it folds is fed back and folded again, so it grows into itself.
    id: 'example-kaleido-loop',
    label: 'Kaleidoscope in the loop',
    line: 'Fed back into itself: each fold folded again.',
    routes: {},
    extra: emptyExtra(),
    settings: { ...QUIET, effects: 1, fxLoop: 1, fxLoopK1: 0.4, fxLoopAmount: 0.8, trails: 1 },
  },
  {
    // The layer as a window: the flame seen only through the shapes, dimmed elsewhere.
    id: 'example-window',
    label: 'A window onto the flame',
    line: 'The flame seen through the shapes, dark around them.',
    routes: {},
    extra: {
      ...emptyExtra(),
      layers: [{ id: 'window', source: mod('shapes', 'window-source', { count: 3, sides: 6, radius: 0.3, spread: 0.5, fill: 1, spin: 0.03 }), modules: [mod('glow', 'window-glow', { glow: 0.5 })], loop: [], blend: { mode: 'window', amount: 0.85 } }],
    },
    settings: { ...QUIET },
  },
  {
    // Feedback alone: the picture's own loop moves the last picture a little (Warp) and keeps some of it (Keep).
    id: 'example-feedback',
    label: 'Feedback',
    line: 'The last picture moved a little and kept: trails that zoom and turn.',
    routes: {},
    extra: { ...emptyExtra(), loop: [mod('warp', 'example-warp', { zoom: 0.15, turn: 0.15 }), mod('keep', 'example-keep', { amount: 0.8, trails: 0.6 })] },
    settings: { ...QUIET },
  },
  {
    // A modulator between the music and a knob: the bass's hits through a beat gate (at typical) push the size. One
    // cable in, one out.
    id: 'example-gate',
    label: 'A beat gate',
    line: 'The bass through a gate into the size: a push on each hit.',
    routes: {},
    extra: {
      ...emptyExtra(),
      modulators: [mod('gate', 'kick', { above: 1, amount: 1 })],
      cables: [cable('example-kick-in', { signal: 'hits', band: 1 }, 'kick', 'in', 1), cable('example-size', { modulator: 'kick' }, 'flame', 'zoom', 0.4)],
    },
    settings: { ...QUIET },
  },
  {
    // Time, not the music: a Drift wanders the turning; nothing of the sound moves anything.
    id: 'example-drift',
    label: 'Moved by time',
    line: 'A drift turns it; the music moves nothing.',
    routes: {},
    extra: { ...emptyExtra(), modulators: [mod('random', 'wander', { rate: 0.1, amount: 1 })], cables: [cable('example-turn', { modulator: 'wander' }, 'flame', 'rotation', 0.5)] },
    settings: { ...QUIET },
  },
];

/** From scratch: nothing at all (the owner): no flame, no cables, no extras, the loop and finish off. */
export const SCRATCH: Chain = { id: 'scratch', label: 'Start from scratch', line: 'Nothing: add a source', routes: {}, extra: emptyExtra(), settings: { ...QUIET, flameLayer: 0 } };

/** What a chain is besides its wiring (patch: its modules and cables): whether the flame is drawn, which effect stands
 *  in each slot, and which of the loop's and finish's own are on; not how much (the dice and the knobs move that within
 *  it). The panel names the chain the patch still is by these. */
export function chainShape(x: Partial<Record<string, unknown>>): string {
  const n = (k: string) => (typeof x[k] === 'number' ? (x[k] as number) : 0);
  return [
    n('flameLayer') > 0 ? 'flame' : '',
    ...['fx1', 'fx2', 'fx3', 'fxLoop'].map((k) => `${k}=${n(`${k}Amount`) > 0 ? Math.round(n(k)) : 0}`),
    ...['fluid', 'tunnel', 'dive'].map((k) => (n(k) > 0 ? k : '')),
  ].join(',');
}

export const chainOf = (id: string): Chain | undefined => (id === SCRATCH.id ? SCRATCH : [...CHAINS, ...EXAMPLES].find((c) => c.id === id));

/** A chain's routes as the settings keep them: every default route off (0) but those the chain sets. The settings keep
 *  their routes over the defaults (settings.svelte.ts `keptFrom`), so one left out would come back at the next load. */
export function chainRoutes(c: Chain, bands = 6): Record<string, RouteValue> {
  return { ...Object.fromEntries(Object.keys(defaultRoutes(bands)).map((k) => [k, 0])), ...c.routes };
}
